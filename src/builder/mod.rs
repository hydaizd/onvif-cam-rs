use crate::device::*;
use anyhow::Result;

pub mod camera;

pub trait Builder {
    fn set_capabilities(onvif_url: url::Url, auth: Option<&Auth>) -> Result<Capabilities>;
    fn set_device_info(onvif_url: url::Url, auth: Option<&Auth>) -> Result<DeviceInfo>;
    fn set_profiles(onvif_url: url::Url, auth: Option<&Auth>) -> Result<Profiles>;
    fn set_stream_uri(onvif_url: url::Url, profile_token: &str, auth: Option<&Auth>) -> Result<StreamUri>;
    fn set_services(onvif_url: url::Url, auth: Option<&Auth>);
    fn set_service_capabilities(onvif_url: url::Url, auth: Option<&Auth>);
    fn set_dns(onvif_url: url::Url, auth: Option<&Auth>);
    fn build_all(&mut self);
}
