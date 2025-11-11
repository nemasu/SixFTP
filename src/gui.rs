use iced::{Element, Length, Task, Subscription, Event};
use iced::widget::{button, column, container, row, text, text_input, scrollable, text_editor, Space};
use iced::window;
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;
use anyhow::Result;
use std::path::PathBuf;
use std::net::IpAddr;
use unftp_sbe_fs::ServerExt;
use crate::network_info::ServerInfo;
use tracing::info;

#[derive(Debug, Clone)]
pub enum Message {
    DirectoryChanged(String),
    UsernameChanged(String),
    PasswordChanged(String),
    PortChanged(String),
    PasvRangeChanged(String),
    BindAddressChanged(String),
    StartServer,
    StopServer,
    ServerInfoEdited(text_editor::Action),
    EventOccurred(Event),
}

pub struct SixFtpGui {
    directory: String,
    username: String,
    password: String,
    port: String,
    pasv_range: String,
    bind_address: String,
    server_running: bool,
    server_status: String,
    server_info: text_editor::Content,
    server_status_content: text_editor::Content,
    server_handle: Option<Arc<Mutex<ServerHandle>>>,
}

impl Default for SixFtpGui {
    fn default() -> Self {
        let server_status = "Server not started".to_string();
        Self {
            directory: ".".to_string(),
            username: "user".to_string(),
            password: "password".to_string(),
            port: "9000".to_string(),
            pasv_range: "30000-30100".to_string(),
            bind_address: "0.0.0.0".to_string(),
            server_running: false,
            server_status: server_status.clone(),
            server_info: text_editor::Content::new(),
            server_status_content: text_editor::Content::with_text(&server_status),
            server_handle: None,
        }
    }
}

struct ServerHandle {
    runtime: Runtime,
    server_tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl ServerHandle {
    fn shutdown(self) {
        // Abort all server tasks
        for task in &self.server_tasks {
            task.abort();
        }

        // Intentionally leak the runtime to avoid drop panics
        // This is acceptable for a GUI application where the server stop
        // is typically followed by the application closing anyway
        std::mem::forget(self);
    }
}

impl SixFtpGui {
    fn subscription(&self) -> Subscription<Message> {
        iced::event::listen().map(Message::EventOccurred)
    }

    fn start_server(&mut self) -> Task<Message> {
        if self.server_running {
            return Task::none();
        }

        // Validate inputs
        let port = match self.port.parse::<u16>() {
            Ok(p) => p,
            Err(_) => {
                self.server_status = "Invalid port number".to_string();
                return Task::none();
            }
        };

        let pasv_range = match parse_pasv_range(&self.pasv_range) {
            Ok(range) => range,
            Err(e) => {
                self.server_status = format!("Invalid passive range: {}", e);
                return Task::none();
            }
        };

        // Strip brackets from IPv6 addresses if present
        let bind_address_cleaned = self.bind_address.trim()
            .trim_start_matches('[')
            .trim_end_matches(']');

        let bind_addr: IpAddr = match bind_address_cleaned.parse() {
            Ok(addr) => addr,
            Err(_) => {
                self.server_status = "Invalid bind address".to_string();
                return Task::none();
            }
        };

        let directory = PathBuf::from(&self.directory);
        if !directory.exists() {
            self.server_status = "Directory does not exist".to_string();
            return Task::none();
        }

        // Create a new runtime for the server
        let runtime = Runtime::new().unwrap();

        // Clone values for the async tasks
        let directory_clone = directory.clone();
        let pasv_range_clone = pasv_range.clone();

        let mut server_tasks = Vec::new();

        // If bind address is unspecified, bind to both IPv4 and IPv6
        if bind_addr.is_unspecified() {
            // IPv4 task
            let directory_ipv4 = directory_clone.clone();
            let pasv_range_ipv4 = pasv_range_clone.clone();
            let ipv4_bind = "0.0.0.0".parse::<IpAddr>().unwrap();

            let ipv4_task = runtime.spawn(async move {
                let bind_string = format!("{}:{}", ipv4_bind, port);
                let server = libunftp::Server::with_fs(directory_ipv4)
                    .passive_ports(pasv_range_ipv4)
                    .passive_host(libunftp::options::PassiveHost::FromConnection)
                    .greeting("Welcome to SixFTP Server")
                    .build()
                    .unwrap();

                if let Err(e) = server.listen(bind_string).await {
                    eprintln!("IPv4 server error: {}", e);
                }
            });
            server_tasks.push(ipv4_task);

            // IPv6 task
            let directory_ipv6 = directory_clone.clone();
            let pasv_range_ipv6 = pasv_range_clone.clone();
            let ipv6_bind = "::".parse::<IpAddr>().unwrap();

            let ipv6_task = runtime.spawn(async move {
                let bind_string = format!("[{}]:{}", ipv6_bind, port);
                let server = libunftp::Server::with_fs(directory_ipv6)
                    .passive_ports(pasv_range_ipv6)
                    .passive_host(libunftp::options::PassiveHost::FromConnection)
                    .greeting("Welcome to SixFTP Server")
                    .build()
                    .unwrap();

                if let Err(e) = server.listen(bind_string).await {
                    eprintln!("IPv6 server error: {}", e);
                }
            });
            server_tasks.push(ipv6_task);
        } else {
            // Use the specified bind address
            let bind_string = if bind_addr.is_ipv6() {
                format!("[{}]:{}", bind_addr, port)
            } else {
                format!("{}:{}", bind_addr, port)
            };

            let server_task = runtime.spawn(async move {
                let server = libunftp::Server::with_fs(directory_clone)
                    .passive_ports(pasv_range_clone)
                    .passive_host(libunftp::options::PassiveHost::FromConnection)
                    .greeting("Welcome to SixFTP Server")
                    .build()
                    .unwrap();

                if let Err(e) = server.listen(bind_string).await {
                    eprintln!("Server error: {}", e);
                }
            });
            server_tasks.push(server_task);
        }

        let handle = ServerHandle {
            runtime,
            server_tasks,
        };

        self.server_handle = Some(Arc::new(Mutex::new(handle)));
        self.server_running = true;
        self.server_status = "Server running".to_string();

        // Generate comprehensive server information
        let successful_bindings = if bind_addr.is_unspecified() {
            vec![
                "0.0.0.0".parse::<IpAddr>().unwrap(),
                "::".parse::<IpAddr>().unwrap()
            ]
        } else {
            vec![bind_addr]
        };

        info!("GUI: FTP server started successfully on port {} with {} binding(s)", port, successful_bindings.len());
        
        let server_info = ServerInfo {
            successful_bindings,
            port,
            pasv_range,
            directory,
            username: self.username.clone(),
            password: self.password.clone(),
        };
        
        self.server_info = text_editor::Content::with_text(&server_info.format_display_info());

        Task::none()
    }

    fn stop_server(&mut self) -> Task<Message> {
        if !self.server_running {
            return Task::none();
        }

        // Shutdown the server by calling the shutdown method
        if let Some(handle_arc) = self.server_handle.take() {
            // Try to unwrap the Arc - if there are other references, this will just drop our reference
            if let Ok(handle_mutex) = Arc::try_unwrap(handle_arc) {
                if let Ok(handle) = handle_mutex.into_inner() {
                    // Call the shutdown method which will abort tasks and leak the runtime
                    handle.shutdown();
                }
            }
        }

        self.server_running = false;
        self.server_status = "Server stopped".to_string();
        self.server_info = text_editor::Content::new();
        self.server_status_content = text_editor::Content::with_text(&self.server_status);

        info!("GUI: FTP server stopped");

        Task::none()
    }
}

fn parse_pasv_range(range_str: &str) -> Result<std::ops::RangeInclusive<u16>> {
    let parts: Vec<&str> = range_str.split('-').collect();
    if parts.len() != 2 {
        return Err(anyhow::anyhow!("Invalid passive port range format. Use 'start-end'"));
    }

    let start: u16 = parts[0].parse()?;
    let end: u16 = parts[1].parse()?;

    if start > end {
        return Err(anyhow::anyhow!("Passive port range start must be less than or equal to end"));
    }

    if end - start > 100 {
        return Err(anyhow::anyhow!("Passive port range too large. Maximum 100 ports allowed"));
    }

    Ok(start..=end)
}

pub fn update(state: &mut SixFtpGui, message: Message) -> Task<Message> {
    match message {
        Message::DirectoryChanged(dir) => {
            state.directory = dir;
            Task::none()
        }
        Message::UsernameChanged(user) => {
            state.username = user;
            Task::none()
        }
        Message::PasswordChanged(pass) => {
            state.password = pass;
            Task::none()
        }
        Message::PortChanged(port) => {
            state.port = port;
            Task::none()
        }
        Message::PasvRangeChanged(range) => {
            state.pasv_range = range;
            Task::none()
        }
        Message::BindAddressChanged(addr) => {
            state.bind_address = addr;
            Task::none()
        }
        Message::StartServer => state.start_server(),
        Message::StopServer => state.stop_server(),
        Message::ServerInfoEdited(action) => {
            // Allow text selection by performing the action
            // Users can edit the text, but text selection is more important
            state.server_info.perform(action);
            Task::none()
        }
        Message::EventOccurred(event) => {
            // Handle window close request
            if let Event::Window(window::Event::CloseRequested) = event {
                info!("GUI: Window close requested, stopping server gracefully");

                // Stop the server if it's running
                if state.server_running {
                    let _ = state.stop_server();
                }

                // Close the window
                return window::get_latest().and_then(window::close);
            }
            Task::none()
        }
    }
}

pub fn view(state: &SixFtpGui) -> Element<'_, Message> {
    let title = text("SixFTP Server").size(24);

    let directory_input = column![
        text("Directory to serve:"),
        text_input("Directory", &state.directory)
            .on_input(Message::DirectoryChanged)
            .padding(10)
    ].spacing(3);

    let credentials_row = row![
        column![
            text("Username:"),
            text_input("Username", &state.username)
                .on_input(Message::UsernameChanged)
                .padding(10)
        ].spacing(3),
        column![
            text("Password:"),
            text_input("Password", &state.password)
                .on_input(Message::PasswordChanged)
                .padding(10)
        ].spacing(3)
    ].spacing(15);

    let network_row = row![
        column![
            text("Port:"),
            text_input("Port", &state.port)
                .on_input(Message::PortChanged)
                .padding(10)
        ]
        .spacing(3)
        .width(Length::Fill),
        column![
            text("Passive Port Range:"),
            text_input("Passive Port Range", &state.pasv_range)
                .on_input(Message::PasvRangeChanged)
                .padding(10)
        ]
        .spacing(3)
        .width(Length::Fill),
        column![
            text("Bind Address:"),
            text_input("Bind Address", &state.bind_address)
                .on_input(Message::BindAddressChanged)
                .padding(10)
        ]
        .spacing(3)
        .width(Length::Fill)
    ].spacing(15);

    let server_control = if state.server_running {
        button("Stop Server")
            .on_press(Message::StopServer)
    } else {
        button("Start Server")
            .on_press(Message::StartServer)
    };

    let status_box = if state.server_running {
        container(
            scrollable(
                text_editor(&state.server_info)
                    .on_action(Message::ServerInfoEdited)
            )
            .height(Length::Fill)
            .width(Length::Fill)
        )
        .padding(10)
        .width(Length::Fill)
        .height(Length::Fill)

    } else {
        container(
            scrollable(
                text_editor(&state.server_status_content)
                    .on_action(Message::ServerInfoEdited)
            )
            .height(Length::Fill)
            .width(Length::Fill)
        )
        .padding(10)
        .width(Length::Fill)
        .height(Length::Fill)

    };

    let content = column![
        title,
        Space::with_height(15),
        directory_input,
        Space::with_height(8),
        credentials_row,
        Space::with_height(8),
        network_row,
        Space::with_height(20),
        server_control,
        Space::with_height(20),
        text("Server Status:").size(18).style(|_theme| {
            text::Style {
                color: None,
            }
        }),
        Space::with_height(8),
        status_box
    ]
    .spacing(0)
    .padding(20)
    .width(Length::Fill)
    .height(Length::Fill);

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
}

pub fn run_gui() -> Result<()> {
    iced::application("SixFTP Server", update, view)
        .window_size((1100.0, 920.0))
        .subscription(SixFtpGui::subscription)
        .exit_on_close_request(false)
        .run()?;
    Ok(())
}