use std::collections::VecDeque;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use chrono::Local;
use data::buffer::Upstream;
use data::client::{self, Event};
use data::rate_limit::TokenPriority;
use data::{Config, Server, message, server, stream, target};
use futures::stream::{BoxStream, SelectAll};
use futures::{FutureExt, StreamExt};
use irc::proto::{self, Command, command};
use serde::Deserialize;
use tokio::time;

use crate::TuiTerminal;

const MAX_LINES_PER_BUFFER: usize = 5_000;
const CTRL_C_CONFIRM_WINDOW: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TuiConfig {
    pub sidebar: bool,
    pub sidebar_width: u16,
    pub timestamp_format: String,
    pub mouse: bool,
    pub history_dir: Option<String>,
    pub keybinds: crate::keybinds::Keybinds,
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            sidebar: true,
            sidebar_width: 24,
            timestamp_format: "%H:%M".to_string(),
            mouse: false,
            history_dir: None,
            keybinds: crate::keybinds::Keybinds::default(),
        }
    }
}

impl TuiConfig {
    pub async fn load() -> Result<Self> {
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Root {
            tui: TuiConfig,
        }

        let content = tokio::fs::read_to_string(Config::path()).await?;
        let root = toml::from_str::<Root>(&content)?;

        Ok(root.tui)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct BufferView {
    pub upstream: Upstream,
    pub name: String,
    pub lines: VecDeque<MessageLine>,
    pub unread: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct MessageLine {
    pub timestamp: String,
    pub nick: Option<String>,
    pub text: String,
    pub kind: LineKind,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum LineKind {
    Normal,
    Own,
    Notice,
    Action,
    Status,
    Error,
}

pub struct App {
    pub(crate) config: Config,
    pub(crate) servers: server::Map,
    pub(crate) tui_config: TuiConfig,
    pub(crate) clients: client::Map,
    pub(crate) controllers: stream::Map,
    pub(crate) buffers: Vec<BufferView>,
    pub(crate) active: usize,
    pub(crate) input: String,
    pub(crate) scroll_back: usize,
    pub(crate) sidebar_visible: bool,
    pub(crate) status: String,
    pub(crate) compose_mode: bool,
    pub(crate) command_palette: bool,
    pub(crate) last_ctrl_c: Option<Instant>,
}

impl App {
    pub fn new(config: Config, tui_config: TuiConfig) -> Self {
        let sidebar_visible = tui_config.sidebar;
        let servers = server::Map::from(config.servers.clone());
        let mut app = Self {
            config,
            servers,
            tui_config,
            clients: client::Map::default(),
            controllers: stream::Map::default(),
            buffers: Vec::new(),
            active: 0,
            input: String::new(),
            scroll_back: 0,
            sidebar_visible,
            status: "starting".to_string(),
            compose_mode: false,
            command_palette: false,
            last_ctrl_c: None,
        };

        app.add_configured_buffers();
        app
    }

    pub async fn run(mut self, terminal: &mut TuiTerminal) -> Result<()> {
        let mut server_streams: SelectAll<BoxStream<'static, stream::Update>> =
            SelectAll::new();

        for entry in self.servers.entries() {
            server_streams
                .push(stream::run(entry, self.config.proxy.clone()).boxed());
        }

        if server_streams.is_empty() {
            return Err(anyhow!("config has no servers"));
        }

        let mut crossterm_events = crossterm::event::EventStream::new();
        let mut tick = time::interval(Duration::from_secs(1));
        let mut needs_redraw = true;

        loop {
            if needs_redraw {
                terminal.draw(|frame| crate::ui::render(frame, &self))?;
            }

            let (should_quit, redraw) = tokio::select! {
                Some(event) = crossterm_events.next() => {
                    let should_quit = match event {
                        Ok(event) => self.handle_terminal_event(event)?,
                        Err(error) => {
                            self.status = format!("terminal event error: {error}");
                            false
                        }
                    };
                    (should_quit, true)
                }
                Some(update) = server_streams.next() => {
                    self.handle_stream_update(update);
                    while let Some(Some(update)) = server_streams.next().now_or_never() {
                        self.handle_stream_update(update);
                    }
                    (false, true)
                }
                _ = tick.tick() => {
                    if let Err(error) = self.clients.tick(Instant::now()) {
                        self.status = format!("tick failed: {error}");
                        (false, true)
                    } else {
                        (false, false)
                    }
                }
                result = tokio::signal::ctrl_c() => {
                    if let Err(error) = result {
                        self.status = format!("signal error: {error}");
                        (false, true)
                    } else {
                        self.request_quit(None);
                        (true, true)
                    }
                }
            };

            needs_redraw = redraw;

            if should_quit {
                break;
            }
        }

        Ok(())
    }

    fn add_configured_buffers(&mut self) {
        let entries = self.servers.entries().collect::<Vec<_>>();
        let mut first_channel = None;

        for entry in entries {
            let server = entry.server;
            self.ensure_buffer(Upstream::Server(server.clone()));

            for channel in &entry.config.channels {
                let channel = self.parse_channel(&server, channel);
                let index = self
                    .ensure_buffer(Upstream::Channel(server.clone(), channel));
                first_channel.get_or_insert(index);
            }
        }

        if let Some(index) = first_channel {
            self.active = index;
        }
    }

    pub(crate) fn ensure_buffer(&mut self, upstream: Upstream) -> usize {
        let key = upstream.key();
        if let Some(index) = self
            .buffers
            .iter()
            .position(|buffer| buffer.upstream.key() == key)
        {
            return index;
        }

        let name = buffer_name(&upstream);
        self.buffers.push(BufferView {
            upstream,
            name,
            lines: VecDeque::new(),
            unread: 0,
        });
        self.buffers.len() - 1
    }

    pub(crate) fn switch_to(&mut self, index: usize) {
        if index < self.buffers.len() {
            self.active = index;
            self.buffers[index].unread = 0;
            self.scroll_back = 0;
            self.command_palette = false;
        }
    }

    pub(crate) fn switch_relative(&mut self, delta: isize) {
        if self.buffers.is_empty() {
            return;
        }

        let len = self.buffers.len() as isize;
        let next = (self.active as isize + delta).rem_euclid(len) as usize;
        self.switch_to(next);
    }

    pub(crate) fn active_upstream(&self) -> Option<&Upstream> {
        self.buffers.get(self.active).map(|buffer| &buffer.upstream)
    }

    pub(crate) fn active_server(&self) -> Option<Server> {
        self.active_upstream()
            .map(|upstream| upstream.server().clone())
            .or_else(|| self.servers.keys().next().cloned())
    }

    pub(crate) fn add_line(&mut self, upstream: Upstream, line: MessageLine) {
        let index = self.ensure_buffer(upstream);
        let buffer = &mut self.buffers[index];

        if buffer.lines.len() >= MAX_LINES_PER_BUFFER {
            buffer.lines.pop_front();
        }

        buffer.lines.push_back(line);

        if index != self.active {
            buffer.unread = buffer.unread.saturating_add(1);
        }
    }

    pub(crate) fn add_status_line(
        &mut self,
        server: &Server,
        text: impl Into<String>,
    ) {
        let text = text.into();
        self.status = text.clone();
        self.add_line(
            Upstream::Server(server.clone()),
            MessageLine {
                timestamp: self.timestamp(),
                nick: None,
                text,
                kind: LineKind::Status,
            },
        );
    }

    pub(crate) fn add_error_line(
        &mut self,
        server: &Server,
        text: impl Into<String>,
    ) {
        let text = text.into();
        self.status = text.clone();
        self.add_line(
            Upstream::Server(server.clone()),
            MessageLine {
                timestamp: self.timestamp(),
                nick: None,
                text,
                kind: LineKind::Error,
            },
        );
    }

    pub(crate) fn submit_input(&mut self) -> Result<bool> {
        let text = std::mem::take(&mut self.input);
        let trimmed = text.trim();

        if trimmed.is_empty() {
            return Ok(false);
        }

        if let Some(command) = trimmed.strip_prefix('/') {
            self.handle_command(command)
        } else {
            self.send_text_message(text);
            Ok(false)
        }
    }

    pub(crate) fn send_text_message(&mut self, text: String) {
        let Some(upstream) = self.active_upstream().cloned() else {
            self.status = "no active buffer".to_string();
            return;
        };

        let Some(target) = upstream.target() else {
            self.status =
                "select a channel or query before sending".to_string();
            return;
        };

        let target_name = target.as_str().to_string();
        self.clients.send(
            &upstream,
            command!("PRIVMSG", target_name, text.clone()).into(),
            TokenPriority::User,
        );

        self.add_own_line(upstream, text, LineKind::Own);
    }

    pub(crate) fn send_action_message(&mut self, text: String) {
        let Some(upstream) = self.active_upstream().cloned() else {
            self.status = "no active buffer".to_string();
            return;
        };

        let Some(target) = upstream.target() else {
            self.status =
                "select a channel or query before sending".to_string();
            return;
        };

        let payload = format!("\u{1}ACTION {text}\u{1}");
        self.clients.send(
            &upstream,
            command!("PRIVMSG", target.as_str().to_string(), payload).into(),
            TokenPriority::User,
        );

        self.add_own_line(upstream, text, LineKind::Action);
    }

    fn add_own_line(
        &mut self,
        upstream: Upstream,
        text: String,
        kind: LineKind,
    ) {
        let nick = upstream
            .server()
            .clone()
            .pipe(|server| self.own_nick(&server));

        self.add_line(
            upstream,
            MessageLine {
                timestamp: self.timestamp(),
                nick: Some(nick),
                text,
                kind,
            },
        );
        self.scroll_back = 0;
    }

    fn handle_command(&mut self, command: &str) -> Result<bool> {
        let mut parts = command.split_whitespace();
        let Some(name) = parts.next() else {
            return Ok(false);
        };

        match name {
            "join" | "j" => {
                let Some(server) = self.active_server() else {
                    self.status = "no server available".to_string();
                    return Ok(false);
                };
                let Some(channel_name) = parts.next() else {
                    self.status = "usage: /join #channel".to_string();
                    return Ok(false);
                };

                let channel = self.parse_channel(&server, channel_name);
                self.clients.join(&server, std::slice::from_ref(&channel));
                let index = self
                    .ensure_buffer(Upstream::Channel(server.clone(), channel));
                self.switch_to(index);
                self.add_status_line(
                    &server,
                    format!("joining {channel_name}"),
                );
            }
            "part" => {
                let Some(server) = self.active_server() else {
                    self.status = "no server available".to_string();
                    return Ok(false);
                };

                let args = parts.collect::<Vec<_>>();
                let (channel_name, reason) = match args.first().copied() {
                    Some(first)
                        if first.starts_with(channel_prefixes(
                            &self.clients,
                            &server,
                        )) =>
                    {
                        (
                            first.to_string(),
                            args.get(1..).unwrap_or_default().join(" "),
                        )
                    }
                    _ => {
                        let Some(Upstream::Channel(_, channel)) =
                            self.active_upstream()
                        else {
                            self.status =
                                "usage: /part [#channel] [reason]".to_string();
                            return Ok(false);
                        };
                        (channel.as_str().to_string(), args.join(" "))
                    }
                };

                let reason = (!reason.is_empty()).then_some(reason);
                let channel = self.parse_channel(&server, &channel_name);
                let upstream = Upstream::Channel(server.clone(), channel);
                let message = match reason {
                    Some(reason) => {
                        command!("PART", channel_name.clone(), reason)
                    }
                    None => command!("PART", channel_name.clone()),
                };
                self.clients.send(
                    &upstream,
                    message.into(),
                    TokenPriority::User,
                );
                self.add_status_line(
                    &server,
                    format!("parting {channel_name}"),
                );
            }
            "msg" | "query" => {
                let Some(server) = self.active_server() else {
                    self.status = "no server available".to_string();
                    return Ok(false);
                };
                let Some(target_name) = parts.next() else {
                    self.status = "usage: /msg nick message".to_string();
                    return Ok(false);
                };
                let text = parts.collect::<Vec<_>>().join(" ");
                if text.is_empty() {
                    self.status = "usage: /msg nick message".to_string();
                    return Ok(false);
                }

                let query = self.parse_query(&server, target_name);
                let upstream = Upstream::Query(server.clone(), query);
                let index = self.ensure_buffer(upstream.clone());
                self.clients.send(
                    &upstream,
                    command!("PRIVMSG", target_name.to_string(), text.clone())
                        .into(),
                    TokenPriority::User,
                );
                self.add_own_line(upstream, text, LineKind::Own);
                self.switch_to(index);
            }
            "me" => {
                let text = parts.collect::<Vec<_>>().join(" ");
                if text.is_empty() {
                    self.status = "usage: /me action".to_string();
                } else {
                    self.send_action_message(text);
                }
            }
            "ml" => {
                self.compose_mode = true;
                self.status = "compose mode: Enter inserts newline, Ctrl+D sends, Esc cancels".to_string();
            }
            "quit" | "q" => {
                let reason = parts.collect::<Vec<_>>().join(" ");
                self.request_quit((!reason.is_empty()).then_some(reason));
                return Ok(true);
            }
            _ => {
                self.status = format!("unknown command: /{name}");
            }
        }

        Ok(false)
    }

    pub(crate) fn request_quit(&mut self, reason: Option<String>) {
        self.controllers.exit(&reason);
        self.status = "quitting".to_string();
    }

    pub(crate) fn handle_ctrl_c(&mut self) -> bool {
        let now = Instant::now();
        if self.last_ctrl_c.is_some_and(|last| {
            now.duration_since(last) <= CTRL_C_CONFIRM_WINDOW
        }) {
            self.request_quit(None);
            true
        } else {
            self.last_ctrl_c = Some(now);
            self.status = "press Ctrl+C again to quit".to_string();
            false
        }
    }

    pub(crate) fn handle_stream_update(&mut self, update: stream::Update) {
        match update {
            stream::Update::Controller { server, controller } => {
                self.controllers.insert(server.clone(), controller);
                self.add_status_line(&server, "controller ready");
            }
            stream::Update::Connecting { server, .. } => {
                self.add_status_line(&server, "connecting");
            }
            stream::Update::Connected { server, client, .. } => {
                self.clients.ready(server.clone(), client);
                self.add_status_line(&server, "connected");
            }
            stream::Update::Disconnected { server, error, .. } => {
                self.clients.disconnected(server.clone());
                if let Some(error) = error {
                    self.add_error_line(
                        &server,
                        format!("disconnected: {error}"),
                    );
                } else {
                    self.add_status_line(&server, "disconnected");
                }
            }
            stream::Update::ConnectionFailed { server, error, .. } => {
                self.add_error_line(
                    &server,
                    format!("connection failed: {error}"),
                );
            }
            stream::Update::MessagesReceived(server, messages) => {
                for message in messages {
                    let events = match self
                        .clients
                        .receive(&server, message, &self.config)
                        .with_context(|| format!("[{server}] receive failed"))
                    {
                        Ok(events) => events,
                        Err(error) => {
                            self.add_error_line(&server, error.to_string());
                            Vec::new()
                        }
                    };

                    for event in events {
                        self.handle_client_event(&server, event);
                    }
                }
            }
            stream::Update::Remove(server) => {
                self.controllers.remove(&server);
                self.clients.remove(&server);
                self.add_status_line(&server, "removed");
            }
            stream::Update::UpdateConfiguration {
                server,
                updated_config,
            } => {
                for event in
                    self.clients.update_config(&server, updated_config, false)
                {
                    self.handle_client_event(&server, event);
                }
            }
        }
    }

    fn handle_client_event(&mut self, server: &Server, event: Event) {
        match event {
            Event::Single(encoded, _)
            | Event::PrivOrNotice(encoded, _, _)
            | Event::Reaction(encoded) => {
                let upstream = self.infer_upstream(server, &encoded);
                self.add_encoded_line(upstream, encoded);
            }
            Event::WithTarget(encoded, _, target) => {
                let upstream =
                    self.upstream_from_message_target(server, target);
                self.add_encoded_line(upstream, encoded);
            }
            Event::DirectMessage(encoded, _, user) => {
                let query = target::Query::from(&user);
                self.add_encoded_line(
                    Upstream::Query(server.clone(), query),
                    encoded,
                );
            }
            Event::Broadcast(broadcast) => {
                self.add_status_line(server, format!("{broadcast:?}"));
            }
            Event::JoinedChannel(channel, _) => {
                self.ensure_buffer(Upstream::Channel(
                    server.clone(),
                    channel.clone(),
                ));
                self.add_status_line(server, format!("joined {channel}"));
            }
            Event::LoggedIn(_) => {
                self.add_status_line(server, "logged in");
            }
            Event::AddedIsupportParam(_) => {}
            Event::ChatHistoryTargetReceived(target, _)
            | Event::UpdateReadMarker(target, _) => match target {
                target::Target::Channel(channel) => {
                    self.ensure_buffer(Upstream::Channel(
                        server.clone(),
                        channel,
                    ));
                }
                target::Target::Query(query) => {
                    self.ensure_buffer(Upstream::Query(server.clone(), query));
                }
            },
            Event::ChatHistoryTargetsReceived(_) => {}
            Event::FileTransferRequest(_) => {
                self.add_status_line(
                    server,
                    "file transfer request ignored by TUI",
                );
            }
            Event::MonitoredOnline(users) => {
                self.add_status_line(
                    server,
                    format!("monitored online: {users:?}"),
                );
            }
            Event::MonitoredOffline(users) => {
                self.add_status_line(
                    server,
                    format!("monitored offline: {users:?}"),
                );
            }
            Event::OnConnect(_) => {
                self.add_status_line(
                    server,
                    "on_connect stream ignored by TUI",
                );
            }
            Event::BouncerNetwork(server, _) => {
                self.status = format!("bouncer network discovered: {server}");
            }
            Event::AddToSidebar(query) => {
                self.ensure_buffer(Upstream::Query(server.clone(), query));
            }
            Event::WhoisReady(nick) => {
                self.status = format!("whois ready: {nick}");
            }
            Event::Disconnect(error) => {
                self.controllers.disconnect(server, error);
            }
        }
    }

    fn add_encoded_line(
        &mut self,
        upstream: Upstream,
        encoded: message::Encoded,
    ) {
        let (text, kind) = describe_encoded(&encoded);
        let nick = source_name(&encoded);
        let line = MessageLine {
            timestamp: self.timestamp(),
            nick,
            text,
            kind,
        };

        self.add_line(upstream, line);
        self.scroll_back = 0;
    }

    fn infer_upstream(
        &self,
        server: &Server,
        encoded: &message::Encoded,
    ) -> Upstream {
        match &encoded.command {
            Command::PRIVMSG(target, _)
            | Command::NOTICE(target, _)
            | Command::TAGMSG(target)
            | Command::MARKREAD(target, _) => {
                self.parse_target_upstream(server, target)
            }
            Command::JOIN(channels, _)
            | Command::PART(channels, _)
            | Command::TOPIC(channels, _)
            | Command::NAMES(channels)
            | Command::KICK(channels, _, _) => {
                let channel = channels.split(',').next().unwrap_or(channels);
                Upstream::Channel(
                    server.clone(),
                    self.parse_channel(server, channel),
                )
            }
            _ => Upstream::Server(server.clone()),
        }
    }

    fn upstream_from_message_target(
        &self,
        server: &Server,
        target: message::Target,
    ) -> Upstream {
        match target {
            message::Target::Server { .. } | message::Target::Logs { .. } => {
                Upstream::Server(server.clone())
            }
            message::Target::Channel { channel, .. }
            | message::Target::Highlights { channel, .. } => {
                Upstream::Channel(server.clone(), channel)
            }
            message::Target::Query { query, .. } => {
                Upstream::Query(server.clone(), query)
            }
        }
    }

    fn parse_target_upstream(&self, server: &Server, name: &str) -> Upstream {
        if name.starts_with(channel_prefixes(&self.clients, server)) {
            Upstream::Channel(server.clone(), self.parse_channel(server, name))
        } else {
            Upstream::Query(server.clone(), self.parse_query(server, name))
        }
    }

    fn parse_channel(&self, server: &Server, name: &str) -> target::Channel {
        target::Channel::from_str(
            name,
            channel_prefixes(&self.clients, server),
            self.clients.get_casemapping(server),
        )
    }

    fn parse_query(&self, server: &Server, name: &str) -> target::Query {
        target::Query::parse(
            name,
            channel_prefixes(&self.clients, server),
            self.clients.get_statusmsg(server),
            self.clients.get_casemapping(server),
        )
        .unwrap_or_else(|_| {
            target::Query::from(data::user::Nick::from_str(
                name,
                self.clients.get_casemapping(server),
            ))
        })
    }

    fn own_nick(&self, server: &Server) -> String {
        self.clients
            .nickname(server)
            .map(|nick| nick.as_str().to_string())
            .or_else(|| {
                self.servers
                    .get(server)
                    .map(|config| config.nickname.clone())
            })
            .unwrap_or_else(|| "me".to_string())
    }

    fn timestamp(&self) -> String {
        Local::now()
            .format(&self.tui_config.timestamp_format)
            .to_string()
    }
}

fn channel_prefixes<'a>(
    clients: &'a client::Map,
    server: &Server,
) -> &'a [char] {
    let prefixes = clients.get_chantypes(server);
    if prefixes.is_empty() {
        proto::DEFAULT_CHANNEL_PREFIXES
    } else {
        prefixes
    }
}

fn buffer_name(upstream: &Upstream) -> String {
    match upstream {
        Upstream::Server(server) => server.to_string(),
        Upstream::Channel(_, channel) => channel.as_str().to_string(),
        Upstream::Query(_, query) => query.as_str().to_string(),
    }
}

fn source_name(encoded: &message::Encoded) -> Option<String> {
    match encoded.source.as_ref()? {
        proto::Source::Server(server) => Some(server.clone()),
        proto::Source::User(user) => Some(user.nickname.clone()),
    }
}

fn describe_encoded(encoded: &message::Encoded) -> (String, LineKind) {
    match &encoded.command {
        Command::PRIVMSG(_, text) => {
            if let Some(action) = action_text(text) {
                (action.to_string(), LineKind::Action)
            } else {
                (text.clone(), LineKind::Normal)
            }
        }
        Command::NOTICE(_, text) => (text.clone(), LineKind::Notice),
        Command::JOIN(channels, _) => {
            (format!("joined {channels}"), LineKind::Status)
        }
        Command::PART(channels, reason) => {
            let reason = reason
                .as_ref()
                .map(|reason| format!(": {reason}"))
                .unwrap_or_default();
            (format!("left {channels}{reason}"), LineKind::Status)
        }
        Command::QUIT(reason) => {
            let reason = reason
                .as_ref()
                .map(|reason| format!(": {reason}"))
                .unwrap_or_default();
            (format!("quit{reason}"), LineKind::Status)
        }
        Command::NICK(nick) => {
            (format!("is now known as {nick}"), LineKind::Status)
        }
        Command::TOPIC(channel, Some(topic)) => (
            format!("changed topic for {channel}: {topic}"),
            LineKind::Status,
        ),
        Command::TOPIC(channel, None) => {
            (format!("requested topic for {channel}"), LineKind::Status)
        }
        Command::KICK(channel, victim, reason) => {
            let reason = reason
                .as_ref()
                .map(|reason| format!(": {reason}"))
                .unwrap_or_default();
            (
                format!("kicked {victim} from {channel}{reason}"),
                LineKind::Status,
            )
        }
        Command::ERROR(error) => (error.clone(), LineKind::Error),
        Command::FAIL(command, code, _, description) => (
            format!("{command} failed ({code}): {description}"),
            LineKind::Error,
        ),
        Command::WARN(command, code, _, description) => (
            format!("{command} warning ({code}): {description}"),
            LineKind::Notice,
        ),
        Command::NOTE(command, code, _, description) => (
            format!("{command} note ({code}): {description}"),
            LineKind::Status,
        ),
        Command::Numeric(numeric, params) => {
            let text = params
                .last()
                .cloned()
                .unwrap_or_else(|| format!("{params:?}"));
            (format!("{numeric:?}: {text}"), LineKind::Status)
        }
        command => {
            let command_name = command.command();
            let params = command.clone().parameters().join(" ");
            if params.is_empty() {
                (command_name.into_owned(), LineKind::Status)
            } else {
                (format!("{command_name} {params}"), LineKind::Status)
            }
        }
    }
}

fn action_text(text: &str) -> Option<&str> {
    text.strip_prefix("\u{1}ACTION ")
        .and_then(|text| text.strip_suffix('\u{1}'))
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}

impl<T> Pipe for T {}
