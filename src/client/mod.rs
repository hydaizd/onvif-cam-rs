use crate::device::{parse_device_type, Auth, Device};
use crate::utils::parse_soap;

use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use log::trace;
use rand::Rng;
use reqwest::Response;
use sha1::{Digest, Sha1};
use std::sync::OnceLock;
use std::{net::SocketAddr, time::Duration};
use tokio::{net::UdpSocket, time::timeout};
use url::Url;
use uuid::Uuid;

const DISCOVER_URI: &'static str = "239.255.255.250:3702";
const CLIENT_LISTEN_IP: &'static str = "0.0.0.0:0"; // notice port is 0
const MAX_RETRIES: usize = 5;
const TIMEOUT_SECS: u64 = 1;

/// All of the ONVIF requests that this program plans to support
#[derive(Debug)]
pub enum Messages {
    Discovery,
    Capabilities,
    DeviceInfo,
    Profiles,
    GetStreamURI(String),
    GetServices, // a summarized version of Capabilities
    GetServiceCapabilities,
    GetDNS,
    GetNetworkInterfaces,
    GetNetworkProtocols,
    GetNetworkDefaultGateway,
    GetDot11Capabilities,
    GetDot11Status,
    GetSystemUris,
    GetSystemLog,
    GetDiscoveryMode,
    GetGeoLocation,
    GetStorageConfigurations,
    CreatePullPointSubscriptionRequest,
    GetAnalyticsConfigurations,
    GetEventProperties,
    GetProfiles,
    GetEventBrokers,
    PullMessages,
}

/// Sends a multicast request via raw udpsocket on LAN.
/// Request is in the form of a SOAP message.
/// Response is also a SOAP message that will contain
/// the xaddrs of the all the responding devices. Each xaddrs
/// is a URI to subsequently send ONVIF messages
///
/// # Examples
///
/// ```
/// // Find all IP Devices on local network using ONVIF
/// let mut devices = client::discover().await?;
/// let mut cameras: Vec<Camera> = Vec::new();
///
/// ```
pub async fn discover() -> Result<Vec<Device>> {
    // Discovery is based on ws-discovery
    // Which allows for TCP or UDP
    // We will use a raw UDP socket
    let addr_listen: Result<SocketAddr, _> = CLIENT_LISTEN_IP.parse();
    let addr_listen = match addr_listen {
        Ok(addr) => addr,
        Err(e) => panic!("[OnvifClient][Discover] Error creating listen address: {e}"),
    };

    let addr_send: Result<SocketAddr, _> = DISCOVER_URI.parse();
    let addr_send = match addr_send {
        Ok(addr) => addr,
        Err(e) => panic!("[OnvifClient][Discover] Error creating send address: {e}"),
    };

    // Bind to "0.0.0.0" by default
    // This is to receive incoming replies
    let udp_client = UdpSocket::bind(addr_listen).await?;

    // Get the XML SOAP message to broadcast
    let uuid = Uuid::new_v4();
    let msg_discover = soap_msg(&Messages::Discovery, uuid, None);

    // Get responses to broadcast message
    let mut devices_found: Vec<Device> = Vec::new();
    let mut devices_check = String::new();
    let mut try_send = 0;

    while try_send < 2 {
        let mut try_recv = 0;
        try_send += 1;

        // Send the SOAP message over UDP
        // Use default IP and Port
        udp_client.send_to(msg_discover.as_ref(), addr_send).await?;

        while try_recv < 5 {
            try_recv += 1;
            let mut buf = Vec::with_capacity(4096);

            // Wait 1 sec for a response
            if let Ok(recv) = timeout(
                Duration::from_millis(2000),
                udp_client.recv_buf_from(&mut buf),
            )
            .await
            {
                match recv {
                    Ok((size, addr)) => {
                        println!("[OnvifClient][Discover] Received response from: {addr}");

                        if !devices_check.contains(&addr.to_string()) {
                            println!("[OnvifClient][Discover] Found a new device: {addr}");
                            println!("[OnvifClient][Discover] Size of response: {size}");

                            // Add to list of devices already found
                            devices_check = format!("{devices_check}:{addr}");

                            // The SOAP response should provide an XAddrs which will be the
                            // ONVIF URL of the device that responded
                            let xaddrs = parse_soap(&buf[..size], "XAddrs", None, true, false);
                            let url_onvif: Url = xaddrs[0].parse()?;

                            // Get device type
                            let mut device_type =
                                parse_soap(&buf[..size], "Types", None, true, false);
                            let device_type = parse_device_type(device_type.remove(0));

                            // Get scope list
                            let scopes = parse_soap(&buf[..size], "Scopes", None, true, false);
                            let scopes = scopes[0]
                                .split(' ')
                                .map(|s| s.to_string())
                                .collect::<Vec<String>>();

                            devices_found.push(Device {
                                url_onvif,
                                device_type,
                                scopes,
                                auth: None,
                            });
                        }
                    }
                    Err(e) => eprintln!("[OnvifClient][Discover] Error in response {e}"),
                }
            }
        }
    }

    if devices_found.is_empty() {
        panic!("[OnvifClient][Discover] Unable to find any devices.");
    }

    Ok(devices_found)
}

/// Returns the response received when sending an ONVIF request to a
/// device found via device discovery.
/// The response is SOAP formatted as byte array.
///
/// When `auth` is provided, WS-Security UsernameToken (password digest)
/// is automatically added to the SOAP header.
///
/// # Arguments
///
/// * `onvif_url` - The main ONVIF service URL to the device
/// * `msg` - The SOAP request as Messages Enum
/// * `auth` - Optional authentication credentials
///
/// # Examples
///
/// ```
/// let auth = Auth::new("admin", "password");
/// let response = client::send(onvif_url, Messages::GetStreamURI("profile_token".into()), Some(&auth)).await?;
/// ```
pub async fn send(onvif_url: url::Url, msg: Messages, auth: Option<&Auth>) -> Result<Response> {
    // Only generate uuid for Discovery messages that actually use it
    let uuid = match msg {
        Messages::Discovery => Uuid::new_v4(),
        _ => Uuid::nil(),
    };
    let soap_msg = soap_msg(&msg, uuid, auth);
    let client = reqwest_client();

    for attempt in 1..=MAX_RETRIES {
        let request = client
            .post(onvif_url.as_str())
            .header("Content-Type", "application/soap+xml; charset=utf-8")
            .body(soap_msg.clone());

        match timeout(Duration::from_secs(TIMEOUT_SECS), request.send()).await {
            Ok(Ok(response)) => {
                trace!("SOAP reply for {msg:?}: {response:?}");
                return Ok(response);
            }
            Ok(Err(e)) => {
                eprintln!("[Client][send] Request error (attempt {attempt}/{MAX_RETRIES}): {e}");
            }
            Err(_) => {
                eprintln!(
                    "[Client][send] Timeout waiting for response (attempt {attempt}/{MAX_RETRIES})"
                );
            }
        }

        // Small backoff before retry (except after last attempt)
        if attempt < MAX_RETRIES {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    Err(anyhow!(
        "[Client] Failed to get response after {MAX_RETRIES} attempts"
    ))
}

/// Returns a shared reqwest::Client with sensible defaults for ONVIF communication.
fn reqwest_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(5)
            .build()
            .expect("Failed to create reqwest client")
    })
}

/// Generate the WS-Security UsernameToken header with password digest
/// as required by ONVIF specification.
///
/// Digest = Base64(SHA1(Nonce + Created + Password))
fn ws_security_header(username: &str, password: &str) -> String {
    let mut rng = rand::thread_rng();

    // Generate 16 random bytes for Nonce
    let mut nonce_bytes = [0u8; 16];
    rng.fill(&mut nonce_bytes);
    let nonce_b64 = BASE64.encode(&nonce_bytes);

    // Created timestamp in UTC (ISO 8601)
    let created = chrono_countdown();

    // Password Digest: Base64(SHA1(nonce_bytes + created + password))
    let mut digest_input = nonce_bytes.to_vec();
    digest_input.extend_from_slice(created.as_bytes());
    digest_input.extend_from_slice(password.as_bytes());
    let digest = BASE64.encode(Sha1::digest(&digest_input));

    format!(
        r#"<Header>
        <Security s:mustUnderstand="1" xmlns="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd">
            <UsernameToken>
                <Username>{username}</Username>
                <Password Type="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-username-token-profile-1.0#PasswordDigest">{digest}</Password>
                <Nonce EncodingType="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-soap-message-security-1.0#Base64Binary">{nonce_b64}</Nonce>
                <Created xmlns="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-utility-1.0.xsd">{created}</Created>
            </UsernameToken>
        </Security>
    </Header>"#
    )
}

/// Generate a simple UTC timestamp string for WS-Security.
/// Uses std::time to avoid pulling in chrono dependency.
fn chrono_countdown() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    // Manual ISO 8601 formatting: YYYY-MM-DDTHH:MM:SSZ
    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Calculate year/month/day from days since epoch
    let mut y = 1970i64;
    let mut d = days_since_epoch as i64;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if d < days_in_year {
            break;
        }
        d -= days_in_year;
        y += 1;
    }
    let m = month_from_day_of_year(d, is_leap(y));
    let day = d - days_before_month(m, is_leap(y)) + 1;

    format!("{y:04}-{m:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn days_before_month(month: i64, leap: bool) -> i64 {
    match month {
        1 => 0,
        2 => 31,
        3 => if leap { 60 } else { 59 },
        4 => if leap { 91 } else { 90 },
        5 => if leap { 121 } else { 120 },
        6 => if leap { 152 } else { 151 },
        7 => if leap { 182 } else { 181 },
        8 => if leap { 213 } else { 212 },
        9 => if leap { 244 } else { 243 },
        10 => if leap { 274 } else { 273 },
        11 => if leap { 305 } else { 304 },
        12 => if leap { 335 } else { 334 },
        _ => 0,
    }
}

fn month_from_day_of_year(doy: i64, leap: bool) -> i64 {
    for m in 1..=12 {
        if doy < days_before_month(m + 1, leap) {
            return m;
        }
    }
    12
}

pub fn soap_msg(msg_type: &Messages, uuid: Uuid, auth: Option<&Auth>) -> String {
    let prefix = r#"<Envelope xmlns="http://www.w3.org/2003/05/soap-envelope"
                         xmlns:tds="http://www.onvif.org/ver10/device/wsdl">"#;

    let prefix_profiles = r#"<Envelope xmlns="http://www.w3.org/2003/05/soap-envelope"
                         xmlns:trt="http://www.onvif.org/ver10/media/wsdl">"#;

    let prefix_stream_uri = r#"<Envelope xmlns="http://www.w3.org/2003/05/soap-envelope"
                         xmlns:trt="http://www.onvif.org/ver10/media/wsdl"
                         xmlns:tt="http://www.onvif.org/ver10/schema">"#;

    let prefix_discovery_pt1 = r#"<?xml version="1.0" encoding="UTF-8"?>
                        <e:Envelope xmlns:e="http://www.w3.org/2003/05/soap-envelope"
                        xmlns:w="http://schemas.xmlsoap.org/ws/2004/08/addressing"
                        xmlns:d="http://schemas.xmlsoap.org/ws/2005/04/discovery"
                        xmlns:dn="http://www.onvif.org/ver10/network/wsdl">"#;

    // Insert UUID in the MessageID here
    let header_pt1 = format!("<e:Header><w:MessageID>uuid:{uuid}</w:MessageID>");
    let header_pt2 = r#"<w:To>urn:schemas-xmlsoap-org:ws:2005:04:discovery</w:To>
                     <w:Action>http://schemas.xmlsoap.org/ws/2005/04/discovery/Probe</w:Action>
                     </e:Header>"#;

    let body_open = "<Body>";
    let suffix = "</Body></Envelope>";
    let suffix_discovery = r#"<e:Body>
                                   <d:Probe>
                                       <d:Types>dn:NetworkVideoTransmitter</d:Types>
                                   </d:Probe>
                               </e:Body>
                           </e:Envelope>"#;

    let stream_prefix = r#"<trt:GetStreamUri>"#;
    let stream_suffix = r#"<trt:StreamSetup>
               <tt:Stream>RTP-Unicast</tt:Stream>
               <tt:Transport>
                   <tt:Protocol>RTSP</tt:Protocol>
               </tt:Transport>
           </trt:StreamSetup>
       </trt:GetStreamUri>"#;

    // Build security header if auth is provided
    let security_header = auth.map(|a| ws_security_header(&a.username, &a.password));

    // For non-discovery messages, wrap with possible security header
    let make_envelope = |env_prefix: &str, body: &str| -> String {
        let header = match &security_header {
            Some(sh) => format!("{sh}"),
            None => String::new(),
        };
        format!(
            "
                {env_prefix}
                {header}
                {body_open}
                {body}
                {suffix}
            "
        )
    };

    match msg_type {
        Messages::Discovery => format!(
            "
                {prefix_discovery_pt1}
                {header_pt1}
                {header_pt2}
                {suffix_discovery}
            "
        ),
        Messages::Capabilities => make_envelope(
            prefix,
            r#"<tds:GetCapabilities>
                <tds:Category>All</tds:Category>
                </tds:GetCapabilities>"#,
        ),
        Messages::DeviceInfo => make_envelope(prefix, "<tds:GetDeviceInformation/>"),
        Messages::Profiles => make_envelope(prefix_profiles, "<trt:GetProfiles/>"),
        Messages::GetStreamURI(ref profile_token) => make_envelope(
            prefix_stream_uri,
            &format!(
                "{stream_prefix}
                <trt:ProfileToken>{profile_token}</trt:ProfileToken>
                {stream_suffix}"
            ),
        ),
        Messages::GetServices => make_envelope(
            prefix,
            r#"<tds:GetServices>
                <tds:IncludeCapability>true</tds:IncludeCapability>
                </tds:GetServices>"#,
        ),
        Messages::GetServiceCapabilities => make_envelope(prefix, "<tds:GetServiceCapabilities/>"),
        Messages::GetDNS => make_envelope(prefix, "<tds:GetDNS/>"),
        Messages::GetNetworkInterfaces => make_envelope(prefix, "<tds:GetNetworkInterfaces/>"),
        Messages::GetNetworkProtocols => make_envelope(prefix, "<tds:GetNetworkProtocols/>"),
        Messages::GetNetworkDefaultGateway => {
            make_envelope(prefix, "<tds:GetNetworkDefaultGateway/>")
        }
        // wifi功能
        Messages::GetDot11Capabilities => make_envelope(prefix, "<tds:GetDot11Capabilities/>"),
        // wifi连接状态
        Messages::GetDot11Status => make_envelope(prefix, "<tds:GetDot11Status/>"),
        // 部分设备支持
        Messages::GetSystemUris => make_envelope(prefix, "<tds:GetSystemUris/>"),
        // 部分设备支持
        Messages::GetSystemLog => make_envelope(prefix, "<tds:GetSystemLog/>"),
        Messages::GetDiscoveryMode => make_envelope(prefix, "<tds:GetDiscoveryMode/>"),
        Messages::GetGeoLocation => make_envelope(prefix, "<tds:GetGeoLocation/>"),
        Messages::GetStorageConfigurations => {
            make_envelope(prefix, "<tds:GetStorageConfigurations/>")
        }
        Messages::CreatePullPointSubscriptionRequest => {
            make_envelope(prefix, "<tev:CreatePullPointSubscription/>")
        }
        Messages::GetAnalyticsConfigurations => {
            make_envelope(prefix, "<tns:GetAnalyticsConfigurations/>")
        }
        Messages::GetEventProperties => make_envelope(prefix, "<tds:GetEventProperties/>"),
        Messages::GetProfiles => make_envelope(prefix, "<tr2:GetProfiles/>"),
        Messages::GetEventBrokers => make_envelope(prefix, "<tds:GetEventBrokers/>"),
        Messages::PullMessages => make_envelope(
            prefix,
            r#"<wsnt:PullMessages>
                    <wsnt:Timeout>PT5S</wsnt:Timeout>
                    <wsnt:MessageLimit>10</wsnt:MessageLimit>
                </wsnt:PullMessages>"#,
        ),
    }
}
