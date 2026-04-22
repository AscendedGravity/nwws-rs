use std::collections::VecDeque;
use std::fmt;
use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use quick_xml::Reader;
use quick_xml::escape::unescape;
use quick_xml::events::{BytesStart, BytesText, Event};
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

use crate::ParseError;
use crate::oi::NwwsOiMessage;

pub type Result<T> = std::result::Result<T, OiClientError>;
pub type OiClientResult<T> = Result<T>;

#[derive(Debug)]
pub enum OiClientError {
    Io(io::Error),
    Xml(quick_xml::Error),
    Parse(ParseError),
    Protocol(&'static str),
    Authentication(&'static str),
    Tls(rustls::Error),
    InvalidDnsName,
}

impl fmt::Display for OiClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::Xml(err) => write!(f, "{err}"),
            Self::Parse(err) => write!(f, "{err}"),
            Self::Protocol(detail) => write!(f, "XMPP protocol error: {detail}"),
            Self::Authentication(detail) => write!(f, "XMPP authentication error: {detail}"),
            Self::Tls(err) => write!(f, "{err}"),
            Self::InvalidDnsName => f.write_str("invalid TLS server name"),
        }
    }
}

impl std::error::Error for OiClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Xml(err) => Some(err),
            Self::Parse(err) => Some(err),
            Self::Tls(err) => Some(err),
            Self::Protocol(_) | Self::Authentication(_) | Self::InvalidDnsName => None,
        }
    }
}

impl From<io::Error> for OiClientError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<quick_xml::Error> for OiClientError {
    fn from(value: quick_xml::Error) -> Self {
        Self::Xml(value)
    }
}

impl From<ParseError> for OiClientError {
    fn from(value: ParseError) -> Self {
        Self::Parse(value)
    }
}

impl From<rustls::Error> for OiClientError {
    fn from(value: rustls::Error) -> Self {
        Self::Tls(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OiClientConfig {
    pub host: String,
    pub domain: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub resource: String,
    pub room: String,
    pub room_service: String,
    pub nickname: String,
    pub room_password: Option<String>,
    pub history_stanzas: u32,
    pub connect_timeout: Duration,
    pub read_timeout: Option<Duration>,
    pub write_timeout: Option<Duration>,
}

impl OiClientConfig {
    pub fn new(username: impl Into<String>, password: impl Into<String>) -> Self {
        let username = username.into();
        let password = password.into();

        Self {
            host: "nwws-oi.weather.gov".to_owned(),
            domain: "nwws-oi.weather.gov".to_owned(),
            port: 5222,
            username: username.clone(),
            password: password.clone(),
            resource: "nwws".to_owned(),
            room: "nwws".to_owned(),
            room_service: "conference.nwws-oi.weather.gov".to_owned(),
            nickname: username.clone(),
            room_password: Some(password),
            history_stanzas: 0,
            connect_timeout: Duration::from_secs(15),
            read_timeout: Some(Duration::from_secs(30)),
            write_timeout: Some(Duration::from_secs(30)),
        }
    }

    pub fn room_address(&self) -> String {
        format!("{}@{}", self.room, self.room_service)
    }

    pub fn room_jid(&self) -> String {
        format!("{}/{}", self.room_address(), self.nickname)
    }
}

pub fn initial_stream_open(config: &OiClientConfig) -> String {
    format!(
        concat!(
            "<?xml version='1.0'?>",
            "<stream:stream to='{domain}' xmlns='jabber:client' ",
            "xmlns:stream='http://etherx.jabber.org/streams' version='1.0'>"
        ),
        domain = escape_xml_attr(&config.domain)
    )
}

pub fn starttls_stanza() -> &'static str {
    "<starttls xmlns='urn:ietf:params:xml:ns:xmpp-tls'/>"
}

pub fn sasl_plain_auth(config: &OiClientConfig) -> String {
    let payload = format!("\0{}\0{}", config.username, config.password);
    let encoded = base64::engine::general_purpose::STANDARD.encode(payload.as_bytes());
    format!("<auth xmlns='urn:ietf:params:xml:ns:xmpp-sasl' mechanism='PLAIN'>{encoded}</auth>")
}

pub fn bind_iq(resource: &str, id: &str) -> String {
    format!(
        concat!(
            "<iq type='set' id='{id}'>",
            "<bind xmlns='urn:ietf:params:xml:ns:xmpp-bind'>",
            "<resource>{resource}</resource>",
            "</bind>",
            "</iq>"
        ),
        id = escape_xml_attr(id),
        resource = escape_xml_text(resource)
    )
}

pub fn session_iq(domain: &str, id: &str) -> String {
    format!(
        concat!(
            "<iq to='{domain}' type='set' id='{id}'>",
            "<session xmlns='urn:ietf:params:xml:ns:xmpp-session'/>",
            "</iq>"
        ),
        domain = escape_xml_attr(domain),
        id = escape_xml_attr(id)
    )
}

pub fn join_room_presence(config: &OiClientConfig) -> String {
    let mut xml = format!(
        "<presence to='{}'><x xmlns='http://jabber.org/protocol/muc'><history maxstanzas='{}'/>",
        escape_xml_attr(&config.room_jid()),
        config.history_stanzas
    );
    if let Some(password) = config.room_password.as_deref() {
        xml.push_str(&format!(
            "<password>{}</password>",
            escape_xml_text(password)
        ));
    }
    xml.push_str("</x></presence>");
    xml
}

pub trait TlsUpgrader<S> {
    type Secure: Read + Write;

    fn upgrade(self, stream: S, domain: &str) -> Result<Self::Secure>;
}

#[derive(Debug, Clone, Default)]
pub struct RustlsUpgrader;

impl TlsUpgrader<TcpStream> for RustlsUpgrader {
    type Secure = StreamOwned<ClientConnection, TcpStream>;

    fn upgrade(self, stream: TcpStream, domain: &str) -> Result<Self::Secure> {
        let mut roots = RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let config = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let server_name =
            ServerName::try_from(domain.to_owned()).map_err(|_| OiClientError::InvalidDnsName)?;
        let connection = ClientConnection::new(Arc::new(config), server_name)?;
        Ok(StreamOwned::new(connection, stream))
    }
}

#[derive(Debug)]
pub struct NwwsOiClient<S = StreamOwned<ClientConnection, TcpStream>> {
    config: OiClientConfig,
    stream: S,
    reader: XmppReader,
    pending_messages: VecDeque<String>,
    jid: Option<String>,
}

impl NwwsOiClient<StreamOwned<ClientConnection, TcpStream>> {
    pub fn connect(config: OiClientConfig) -> Result<Self> {
        let stream = connect_tcp(&config)?;
        Self::establish_with(stream, config, RustlsUpgrader)
    }

    pub fn establish_with<P, U>(
        stream: P,
        config: OiClientConfig,
        upgrader: U,
    ) -> Result<NwwsOiClient<U::Secure>>
    where
        P: Read + Write,
        U: TlsUpgrader<P>,
    {
        let mut plain = XmppSession::new(stream);
        plain.write_xml(&initial_stream_open(&config))?;
        let features = parse_features(&read_named_fragment(&mut plain, "features")?)?;
        if !features.starttls {
            return Err(OiClientError::Protocol(
                "server did not advertise STARTTLS on port 5222",
            ));
        }

        plain.write_xml(starttls_stanza())?;
        expect_root(&plain.read_fragment()?, "proceed")?;

        let secure = upgrader.upgrade(plain.into_stream(), &config.domain)?;
        let mut session = XmppSession::new(secure);

        session.write_xml(&initial_stream_open(&config))?;
        let auth_features = parse_features(&read_named_fragment(&mut session, "features")?)?;
        if !auth_features.plain_auth {
            return Err(OiClientError::Authentication(
                "server did not advertise SASL PLAIN",
            ));
        }

        session.write_xml(&sasl_plain_auth(&config))?;
        match root_local_name(&session.read_fragment()?)? {
            "success" => {}
            "failure" => {
                return Err(OiClientError::Authentication(
                    "server rejected SASL PLAIN credentials",
                ));
            }
            _ => return Err(OiClientError::Protocol("expected SASL success")),
        }

        session.write_xml(&initial_stream_open(&config))?;
        let bind_features = parse_features(&read_named_fragment(&mut session, "features")?)?;
        if !bind_features.bind {
            return Err(OiClientError::Protocol(
                "server did not advertise XMPP resource binding",
            ));
        }

        session.write_xml(&bind_iq(&config.resource, "bind-1"))?;
        let jid = Some(parse_bind_result(&session.read_fragment()?, "bind-1")?);

        if bind_features.session {
            session.write_xml(&session_iq(&config.domain, "session-2"))?;
            parse_iq_result(&session.read_fragment()?, "session-2")?;
        }

        session.write_xml(&join_room_presence(&config))?;
        let (stream, reader) = session.into_parts();
        let mut client = NwwsOiClient {
            config,
            stream,
            reader,
            pending_messages: VecDeque::new(),
            jid,
        };
        client.wait_for_join()?;
        Ok(client)
    }
}

impl<S> NwwsOiClient<S>
where
    S: Read + Write,
{
    pub fn jid(&self) -> Option<&str> {
        self.jid.as_deref()
    }

    pub fn next_message(&mut self) -> Result<NwwsOiMessage> {
        loop {
            let xml = if let Some(xml) = self.pending_messages.pop_front() {
                xml
            } else {
                self.reader.read_fragment(&mut self.stream)?
            };

            if root_local_name(&xml)? != "message" {
                continue;
            }

            let message = NwwsOiMessage::parse(&xml)?;
            if message.payload.is_some() {
                return Ok(message);
            }
        }
    }

    pub fn close(&mut self) -> Result<()> {
        self.stream.write_all(b"</stream:stream>")?;
        self.stream.flush()?;
        Ok(())
    }

    pub fn into_inner(self) -> S {
        self.stream
    }

    fn wait_for_join(&mut self) -> Result<()> {
        let joined_jid = self.config.room_jid();
        let room_address = self.config.room_address();

        loop {
            let xml = self.reader.read_fragment(&mut self.stream)?;
            match root_local_name(&xml)? {
                "presence" => {
                    let meta = parse_presence_meta(&xml)?;
                    if meta.kind.as_deref() == Some("error") {
                        return Err(OiClientError::Protocol("room join returned presence error"));
                    }
                    if let Some(from) = meta.from.as_deref()
                        && (from == joined_jid || from.starts_with(&(room_address.clone() + "/")))
                    {
                        return Ok(());
                    }
                }
                "message" => self.pending_messages.push_back(xml),
                "failure" => {
                    return Err(OiClientError::Protocol(
                        "server returned failure during room join",
                    ));
                }
                _ => {}
            }
        }
    }
}

struct XmppSession<S> {
    stream: S,
    reader: XmppReader,
}

impl<S> XmppSession<S>
where
    S: Read + Write,
{
    fn new(stream: S) -> Self {
        Self {
            stream,
            reader: XmppReader::default(),
        }
    }

    fn write_xml(&mut self, xml: &str) -> Result<()> {
        self.stream.write_all(xml.as_bytes())?;
        self.stream.flush()?;
        Ok(())
    }

    fn read_fragment(&mut self) -> Result<String> {
        self.reader.read_fragment(&mut self.stream)
    }

    fn into_stream(self) -> S {
        self.stream
    }

    fn into_parts(self) -> (S, XmppReader) {
        (self.stream, self.reader)
    }
}

#[derive(Debug, Default)]
struct XmppReader {
    pending: Vec<u8>,
}

impl XmppReader {
    fn read_fragment<R>(&mut self, stream: &mut R) -> Result<String>
    where
        R: Read,
    {
        loop {
            if let Some((fragment, consumed)) = try_extract_fragment(&self.pending)? {
                self.pending.drain(..consumed);
                return Ok(fragment);
            }

            let mut buffer = [0_u8; 4096];
            let read = stream.read(&mut buffer)?;
            if read == 0 {
                return Err(OiClientError::Io(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "unexpected end of XMPP stream",
                )));
            }
            self.pending.extend_from_slice(&buffer[..read]);
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct StreamFeatures {
    starttls: bool,
    plain_auth: bool,
    bind: bool,
    session: bool,
}

#[derive(Debug, Default)]
struct PresenceMeta {
    from: Option<String>,
    kind: Option<String>,
}

fn connect_tcp(config: &OiClientConfig) -> Result<TcpStream> {
    let mut last_error = None;
    for addr in (config.host.as_str(), config.port).to_socket_addrs()? {
        match TcpStream::connect_timeout(&addr, config.connect_timeout) {
            Ok(stream) => {
                stream.set_nodelay(true)?;
                stream.set_read_timeout(config.read_timeout)?;
                stream.set_write_timeout(config.write_timeout)?;
                return Ok(stream);
            }
            Err(err) => last_error = Some(err),
        }
    }

    Err(OiClientError::Io(last_error.unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::AddrNotAvailable,
            "could not resolve NWWS-OI host",
        )
    })))
}

fn read_named_fragment<S>(session: &mut XmppSession<S>, expected: &str) -> Result<String>
where
    S: Read + Write,
{
    loop {
        let xml = session.read_fragment()?;
        if root_local_name(&xml)? == expected {
            return Ok(xml);
        }
    }
}

fn expect_root(fragment: &str, expected: &str) -> Result<()> {
    if root_local_name(fragment)? == expected {
        Ok(())
    } else {
        Err(OiClientError::Protocol("unexpected XMPP stanza"))
    }
}

fn parse_features(xml: &str) -> Result<StreamFeatures> {
    if root_local_name(xml)? != "features" {
        return Err(OiClientError::Protocol("expected <stream:features> stanza"));
    }

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut features = StreamFeatures::default();
    let mut in_mechanism = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) => match local_name(element.name().as_ref()) {
                "starttls" => features.starttls = true,
                "bind" => features.bind = true,
                "session" => features.session = true,
                "mechanism" => in_mechanism = true,
                _ => {}
            },
            Ok(Event::Empty(element)) => match local_name(element.name().as_ref()) {
                "starttls" => features.starttls = true,
                "bind" => features.bind = true,
                "session" => features.session = true,
                _ => {}
            },
            Ok(Event::Text(text)) if in_mechanism => {
                if decode_text(&text)? == "PLAIN" {
                    features.plain_auth = true;
                }
            }
            Ok(Event::End(element)) if local_name(element.name().as_ref()) == "mechanism" => {
                in_mechanism = false;
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(err) => return Err(err.into()),
        }
    }

    Ok(features)
}

fn parse_bind_result(xml: &str, expected_id: &str) -> Result<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut stanza_type = None;
    let mut stanza_id = None;
    let mut in_jid = false;
    let mut jid = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) => match local_name(element.name().as_ref()) {
                "iq" => {
                    stanza_type = find_attr(&element, b"type")?;
                    stanza_id = find_attr(&element, b"id")?;
                }
                "jid" => in_jid = true,
                _ => {}
            },
            Ok(Event::Text(text)) if in_jid => jid = Some(decode_text(&text)?),
            Ok(Event::End(element)) if local_name(element.name().as_ref()) == "jid" => {
                in_jid = false;
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(err) => return Err(err.into()),
        }
    }

    if stanza_type.as_deref() != Some("result") {
        return Err(OiClientError::Protocol(
            "resource bind did not return iq result",
        ));
    }
    if stanza_id.as_deref() != Some(expected_id) {
        return Err(OiClientError::Protocol(
            "resource bind returned wrong iq id",
        ));
    }

    jid.ok_or(OiClientError::Protocol(
        "resource bind response did not include jid",
    ))
}

fn parse_iq_result(xml: &str, expected_id: &str) -> Result<()> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut stanza_type = None;
    let mut stanza_id = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) | Ok(Event::Empty(element)) => {
                if local_name(element.name().as_ref()) == "iq" {
                    stanza_type = find_attr(&element, b"type")?;
                    stanza_id = find_attr(&element, b"id")?;
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(err) => return Err(err.into()),
        }
    }

    if stanza_type.as_deref() != Some("result") {
        return Err(OiClientError::Protocol("expected iq result stanza"));
    }
    if stanza_id.as_deref() != Some(expected_id) {
        return Err(OiClientError::Protocol("server returned wrong iq id"));
    }

    Ok(())
}

fn parse_presence_meta(xml: &str) -> Result<PresenceMeta> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) | Ok(Event::Empty(element)) => {
                if local_name(element.name().as_ref()) == "presence" {
                    return Ok(PresenceMeta {
                        from: find_attr(&element, b"from")?,
                        kind: find_attr(&element, b"type")?,
                    });
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(err) => return Err(err.into()),
        }
    }

    Err(OiClientError::Protocol("expected presence stanza"))
}

fn find_attr(element: &BytesStart<'_>, key: &[u8]) -> Result<Option<String>> {
    for attr in element.attributes().with_checks(false) {
        let attr = attr.map_err(|_| OiClientError::Protocol("invalid XMPP attribute"))?;
        if attr.key.as_ref() == key {
            let raw = std::str::from_utf8(attr.value.as_ref())
                .map_err(|_| OiClientError::Protocol("invalid UTF-8 attribute"))?;
            let decoded =
                unescape(raw).map_err(|_| OiClientError::Protocol("invalid XML escape"))?;
            return Ok(Some(decoded.into_owned()));
        }
    }
    Ok(None)
}

fn decode_text(text: &BytesText<'_>) -> Result<String> {
    let raw = std::str::from_utf8(text.as_ref())
        .map_err(|_| OiClientError::Protocol("invalid UTF-8 text"))?;
    let decoded = unescape(raw).map_err(|_| OiClientError::Protocol("invalid XML escape"))?;
    Ok(decoded.into_owned())
}

fn root_local_name(xml: &str) -> Result<&str> {
    let start = skip_preamble(xml, 0).ok_or(OiClientError::Protocol(
        "could not locate XMPP root element",
    ))?;
    let tag = xml[start..]
        .strip_prefix('<')
        .ok_or(OiClientError::Protocol("expected XML tag"))?;
    let name_end = tag
        .find(|ch: char| ch.is_ascii_whitespace() || ch == '/' || ch == '>')
        .ok_or(OiClientError::Protocol("malformed XML tag"))?;
    Ok(local_name(&tag.as_bytes()[..name_end]))
}

fn try_extract_fragment(input: &[u8]) -> Result<Option<(String, usize)>> {
    let text = match std::str::from_utf8(input) {
        Ok(text) => text,
        Err(err) if err.error_len().is_none() => return Ok(None),
        Err(_) => return Err(OiClientError::Protocol("server sent invalid UTF-8")),
    };

    let Some(start) = skip_preamble(text, 0) else {
        return Ok(None);
    };

    if text[start..].starts_with("<stream:stream") {
        let Some(end) = find_tag_end(text, start) else {
            return Ok(None);
        };
        return Ok(Some((text[start..end].to_owned(), end)));
    }

    let Some(start_tag_end) = find_tag_end(text, start) else {
        return Ok(None);
    };
    if is_self_closing(text, start, start_tag_end) {
        return Ok(Some((text[start..start_tag_end].to_owned(), start_tag_end)));
    }

    let mut depth = 1usize;
    let mut cursor = start_tag_end;
    while let Some(relative) = text[cursor..].find('<') {
        let tag_start = cursor + relative;

        if text[tag_start..].starts_with("<!--") {
            let Some(end) = text[tag_start + 4..].find("-->") else {
                return Ok(None);
            };
            cursor = tag_start + 4 + end + 3;
            continue;
        }
        if text[tag_start..].starts_with("<![CDATA[") {
            let Some(end) = text[tag_start + 9..].find("]]>") else {
                return Ok(None);
            };
            cursor = tag_start + 9 + end + 3;
            continue;
        }
        if text[tag_start..].starts_with("<?") {
            let Some(end) = text[tag_start + 2..].find("?>") else {
                return Ok(None);
            };
            cursor = tag_start + 2 + end + 2;
            continue;
        }

        let Some(tag_end) = find_tag_end(text, tag_start) else {
            return Ok(None);
        };

        if text[tag_start + 1..].starts_with('/') {
            depth -= 1;
            cursor = tag_end;
            if depth == 0 {
                return Ok(Some((text[start..tag_end].to_owned(), tag_end)));
            }
            continue;
        }

        if !is_self_closing(text, tag_start, tag_end) {
            depth += 1;
        }
        cursor = tag_end;
    }

    Ok(None)
}

fn skip_preamble(input: &str, mut index: usize) -> Option<usize> {
    loop {
        while let Some(ch) = input[index..].chars().next() {
            if ch.is_ascii_whitespace() {
                index += ch.len_utf8();
            } else {
                break;
            }
        }

        if input[index..].starts_with("<?xml") {
            let end = input[index..].find("?>")?;
            index += end + 2;
            continue;
        }
        if input[index..].starts_with("<!--") {
            let end = input[index + 4..].find("-->")?;
            index += 4 + end + 3;
            continue;
        }

        return Some(index);
    }
}

fn find_tag_end(input: &str, start: usize) -> Option<usize> {
    let bytes = input.as_bytes();
    let mut index = start + 1;
    let mut quote = None;

    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(active) = quote {
            if byte == active {
                quote = None;
            }
        } else {
            match byte {
                b'\'' | b'"' => quote = Some(byte),
                b'>' => return Some(index + 1),
                _ => {}
            }
        }
        index += 1;
    }

    None
}

fn is_self_closing(input: &str, start: usize, end: usize) -> bool {
    input[start..end - 1]
        .trim_end_matches(|ch: char| ch.is_ascii_whitespace())
        .ends_with('/')
}

fn local_name(name: &[u8]) -> &str {
    let decoded = std::str::from_utf8(name).unwrap_or_default();
    decoded.rsplit(':').next().unwrap_or(decoded)
}

fn escape_xml_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn escape_xml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::cmp::min;
    use std::io::{self, Read, Write};
    use std::rc::Rc;

    use super::{
        NwwsOiClient, OiClientConfig, Result, TlsUpgrader, bind_iq, initial_stream_open,
        join_room_presence, sasl_plain_auth, session_iq, starttls_stanza,
    };

    const SAMPLE_MESSAGE: &str = "<message to='enduser@server/laptop' type='groupchat' from='nwws@nwws-oi.weather.gov/nwws-oi'><body>KARX issues RR8 valid 2013-05-25T02:20:34Z</body><html xmlns='http://jabber.org/protocol/xhtml-im'><body xmlns='http://www.w3.org/1999/xhtml'>KARX issues RR8 valid 2013-05-25T02:20:34Z</body></html><x xmlns='nwws-oi' cccc='KARX' ttaaii='SRUS83' issue='2013-05-25T02:20:34Z' awipsid='RR8ARX' id='10313.6'>111\r\r\nSRUS83 KARX 250220\r\r\nRR8ARX\r\r\n:\r\r\n: AUTOMATED GAUGE DATA COLLECTED FROM IOWA FLOOD CENTER\r\r\n:\r\r\n.A CDGI4 20130524 C DH2100/HGIRP 2.63 : MORGAN CREEK NEAR CEDAR RAPIDS</x></message>";

    #[test]
    fn builds_expected_stanzas() {
        let config = OiClientConfig::new("demo", "secret");

        assert_eq!(
            initial_stream_open(&config),
            "<?xml version='1.0'?><stream:stream to='nwws-oi.weather.gov' xmlns='jabber:client' xmlns:stream='http://etherx.jabber.org/streams' version='1.0'>"
        );
        assert_eq!(
            starttls_stanza(),
            "<starttls xmlns='urn:ietf:params:xml:ns:xmpp-tls'/>"
        );
        assert_eq!(
            sasl_plain_auth(&config),
            "<auth xmlns='urn:ietf:params:xml:ns:xmpp-sasl' mechanism='PLAIN'>AGRlbW8Ac2VjcmV0</auth>"
        );
        assert_eq!(
            bind_iq("nwws", "bind-1"),
            "<iq type='set' id='bind-1'><bind xmlns='urn:ietf:params:xml:ns:xmpp-bind'><resource>nwws</resource></bind></iq>"
        );
        assert_eq!(
            session_iq("nwws-oi.weather.gov", "session-2"),
            "<iq to='nwws-oi.weather.gov' type='set' id='session-2'><session xmlns='urn:ietf:params:xml:ns:xmpp-session'/></iq>"
        );
        assert_eq!(
            join_room_presence(&config),
            "<presence to='nwws@conference.nwws-oi.weather.gov/demo'><x xmlns='http://jabber.org/protocol/muc'><history maxstanzas='0'/><password>secret</password></x></presence>"
        );
    }

    #[test]
    fn completes_handshake_from_scripted_transcript() {
        let config = OiClientConfig::new("demo", "secret");
        let writes = Rc::new(RefCell::new(Vec::new()));
        let plain_server = [
            "<stream:stream from='nwws-oi.weather.gov' id='1' version='1.0'>",
            "<stream:features><starttls xmlns='urn:ietf:params:xml:ns:xmpp-tls'><required/></starttls></stream:features>",
            "<proceed xmlns='urn:ietf:params:xml:ns:xmpp-tls'/>",
        ]
        .concat();
        let secure_server = [
            "<stream:stream from='nwws-oi.weather.gov' id='2' version='1.0'>",
            "<stream:features><mechanisms xmlns='urn:ietf:params:xml:ns:xmpp-sasl'><mechanism>PLAIN</mechanism></mechanisms></stream:features>",
            "<success xmlns='urn:ietf:params:xml:ns:xmpp-sasl'/>",
            "<stream:stream from='nwws-oi.weather.gov' id='3' version='1.0'>",
            "<stream:features><bind xmlns='urn:ietf:params:xml:ns:xmpp-bind'/><session xmlns='urn:ietf:params:xml:ns:xmpp-session'/></stream:features>",
            "<iq type='result' id='bind-1'><bind xmlns='urn:ietf:params:xml:ns:xmpp-bind'><jid>demo@nwws-oi.weather.gov/nwws</jid></bind></iq>",
            "<iq type='result' id='session-2'/>",
            "<presence from='nwws@conference.nwws-oi.weather.gov/demo'><x xmlns='http://jabber.org/protocol/muc#user'/></presence>",
            SAMPLE_MESSAGE,
        ]
        .concat();
        let transport = ScriptedTransport::fragmented(&plain_server, &[23, 19], Rc::clone(&writes));
        let calls = Rc::new(RefCell::new(Vec::new()));
        let upgrader = MockTlsUpgrader {
            domains: Rc::clone(&calls),
            secure: ScriptedTransport::fragmented(
                &secure_server,
                &[31, 17, 29],
                Rc::clone(&writes),
            ),
        };

        let mut client = NwwsOiClient::establish_with(transport, config, upgrader).unwrap();

        assert_eq!(client.jid(), Some("demo@nwws-oi.weather.gov/nwws"));
        let message = client.next_message().unwrap();
        assert_eq!(message.payload.unwrap().awips_id, "RR8ARX");
        client.close().unwrap();

        let transport = client.into_inner();
        assert_eq!(
            transport.writes_snapshot(),
            vec![
                "<?xml version='1.0'?><stream:stream to='nwws-oi.weather.gov' xmlns='jabber:client' xmlns:stream='http://etherx.jabber.org/streams' version='1.0'>".to_owned(),
                "<starttls xmlns='urn:ietf:params:xml:ns:xmpp-tls'/>".to_owned(),
                "<?xml version='1.0'?><stream:stream to='nwws-oi.weather.gov' xmlns='jabber:client' xmlns:stream='http://etherx.jabber.org/streams' version='1.0'>".to_owned(),
                "<auth xmlns='urn:ietf:params:xml:ns:xmpp-sasl' mechanism='PLAIN'>AGRlbW8Ac2VjcmV0</auth>".to_owned(),
                "<?xml version='1.0'?><stream:stream to='nwws-oi.weather.gov' xmlns='jabber:client' xmlns:stream='http://etherx.jabber.org/streams' version='1.0'>".to_owned(),
                "<iq type='set' id='bind-1'><bind xmlns='urn:ietf:params:xml:ns:xmpp-bind'><resource>nwws</resource></bind></iq>".to_owned(),
                "<iq to='nwws-oi.weather.gov' type='set' id='session-2'><session xmlns='urn:ietf:params:xml:ns:xmpp-session'/></iq>".to_owned(),
                "<presence to='nwws@conference.nwws-oi.weather.gov/demo'><x xmlns='http://jabber.org/protocol/muc'><history maxstanzas='0'/><password>secret</password></x></presence>".to_owned(),
                "</stream:stream>".to_owned(),
            ]
        );
        assert_eq!(calls.borrow().as_slice(), &["nwws-oi.weather.gov"]);
    }

    #[test]
    fn rejects_transcript_without_plain_mechanism() {
        let config = OiClientConfig::new("demo", "secret");
        let writes = Rc::new(RefCell::new(Vec::new()));
        let plain_server = concat!(
            "<stream:stream from='nwws-oi.weather.gov' id='1' version='1.0'>",
            "<stream:features><starttls xmlns='urn:ietf:params:xml:ns:xmpp-tls'><required/></starttls></stream:features>",
            "<proceed xmlns='urn:ietf:params:xml:ns:xmpp-tls'/>"
        );
        let secure_server = concat!(
            "<stream:stream from='nwws-oi.weather.gov' id='2' version='1.0'>",
            "<stream:features><mechanisms xmlns='urn:ietf:params:xml:ns:xmpp-sasl'><mechanism>SCRAM-SHA-256</mechanism></mechanisms></stream:features>"
        );
        let transport = ScriptedTransport::fragmented(plain_server, &[29, 13], Rc::clone(&writes));
        let upgrader = MockTlsUpgrader {
            domains: Rc::new(RefCell::new(Vec::new())),
            secure: ScriptedTransport::fragmented(secure_server, &[17, 7], Rc::clone(&writes)),
        };

        let err = NwwsOiClient::establish_with(transport, config, upgrader)
            .err()
            .unwrap();
        assert!(err.to_string().contains("SASL PLAIN"));
    }

    #[derive(Debug, Default)]
    struct ScriptedTransport {
        reads: Vec<Vec<u8>>,
        read_index: usize,
        read_offset: usize,
        writes: Rc<RefCell<Vec<String>>>,
    }

    impl ScriptedTransport {
        fn fragmented(input: &str, pattern: &[usize], writes: Rc<RefCell<Vec<String>>>) -> Self {
            let bytes = input.as_bytes();
            let mut reads = Vec::new();
            let mut offset = 0usize;
            let mut pattern_index = 0usize;
            while offset < bytes.len() {
                let width = pattern[pattern_index % pattern.len()];
                let end = min(offset + width, bytes.len());
                reads.push(bytes[offset..end].to_vec());
                offset = end;
                pattern_index += 1;
            }
            Self {
                reads,
                read_index: 0,
                read_offset: 0,
                writes,
            }
        }

        fn writes_snapshot(&self) -> Vec<String> {
            self.writes.borrow().clone()
        }
    }

    impl Read for ScriptedTransport {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.read_index >= self.reads.len() {
                return Ok(0);
            }
            let chunk = &self.reads[self.read_index];
            let remaining = &chunk[self.read_offset..];
            let amount = min(buf.len(), remaining.len());
            buf[..amount].copy_from_slice(&remaining[..amount]);
            self.read_offset += amount;
            if self.read_offset >= chunk.len() {
                self.read_index += 1;
                self.read_offset = 0;
            }
            Ok(amount)
        }
    }

    impl Write for ScriptedTransport {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let text = std::str::from_utf8(buf)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid UTF-8"))?;
            self.writes.borrow_mut().push(text.to_owned());
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[derive(Debug)]
    struct MockTlsUpgrader {
        domains: Rc<RefCell<Vec<String>>>,
        secure: ScriptedTransport,
    }

    impl TlsUpgrader<ScriptedTransport> for MockTlsUpgrader {
        type Secure = ScriptedTransport;

        fn upgrade(self, _stream: ScriptedTransport, domain: &str) -> Result<Self::Secure> {
            self.domains.borrow_mut().push(domain.to_owned());
            Ok(self.secure)
        }
    }
}
