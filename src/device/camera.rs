use crate::builder::camera::CameraBuilder;
use crate::device::*;

use anyhow::Result;
use async_trait::async_trait;

#[rustfmt::skip]
pub struct Camera {
    base:                 Device,
    capabilities:         Capabilities,
    profiles:             Profiles,
    device_info:          DeviceInfo,
    pub stream:           StreamUri,
    services:             Services,
    event_props:          EventCapabilities,
    analytics_props:      AnalyticsCapabilities,
    analytics_configs:    AnalyticsConfigList,
    pub auth:             Option<Auth>,
}

#[async_trait]
impl CameraBuilder for Camera {
    #[rustfmt::skip]
    async fn build_all(&mut self) -> Result<()> {
        let auth = self.auth.clone();
        self.capabilities     = Camera::set_capabilities(    self.base.url_onvif.clone(), auth.as_ref()).await?;
        self.device_info      = Camera::set_device_info(     self.base.url_onvif.clone(), auth.as_ref()).await?;
        self.profiles         = Camera::set_profiles(        self.base.url_onvif.clone(), auth.as_ref()).await?;
        self.stream           = Camera::set_stream_uri(      self.base.url_onvif.clone(), self.profiles.token.as_deref().unwrap_or(""), auth.as_ref()).await?;
        self.services         = Camera::set_services(        self.base.url_onvif.clone(), auth.as_ref()).await?;
        // _ =           Camera::set_dot11_status(      self.base.url_onvif.clone(), auth.as_ref()).await?;
        // _ =           Camera::set_geo_location(      self.base.url_onvif.clone(), auth.as_ref()).await?;

        // Get the EVENT SERVICES Url to send request for EVENT capabilities
        // let url            = self.services.event.as_ref().unwrap();
        // let event_url      = url::Url::parse(&url)?;
        // self.event_props   = Camera::set_service_capabilities(event_url, auth.as_ref()).await?;

        // Get the ANALYTICS SERVICES Url to send request for ANALYTICS capabilities
        // let url                 = self.services.analytics.as_ref().unwrap();
        // let analytics_url       = url::Url::parse(&url)?;
        // self.analytics_props    = Camera::set_service_capabilities(analytics_url, auth.as_ref()).await?;

        // Get the MEDIA SERVICES Url to send request for ANALYTICS CONFIGURATIONS
        // let url                     = self.services.media2.as_ref().unwrap();
        // let media_url               = url::Url::parse(&url)?;
        // self.analytics_configs      = Camera::set_analytics_configurations(media_url, auth.as_ref()).await?;

        // Get EVENT SERVICE Url to send request for EVENT PROPERTIES
        // let url                     = self.services.event.as_ref().unwrap();
        // let event_url               = url::Url::parse(&url)?;
        // _      = Camera::set_event_properties(event_url, auth.as_ref()).await?;

        // Get the EVENT SERVICES Url to send request for EVENT pull point
        // let url               = self.services.event.as_ref().unwrap();
        // let event_url         = url::Url::parse(&url)?;
        // _                     = Camera::set_pull_point_sub(event_url, auth.as_ref()).await?;

        // Get the MEDIA2 SERVICE Url to send request for MEDIA2 PROFILES
        // let url               = self.services.media2.as_ref().unwrap();
        // let event_url         = url::Url::parse(&url)?;
        // _                     = Camera::set_service_profiles(event_url, auth.as_ref()).await?;

        // Get the MEDIA2 SERVICE Url to send request for MEDIA2 PROFILES
        // let url               = self.services.event.as_ref().unwrap();
        // let event_url         = url::Url::parse(&url)?;
        // _                     = Camera::set_event_brokers(event_url, auth.as_ref()).await?;

        // Get EVENT SERVICE Url to send request to PULL EVENT MESSAGES
        let url                     = self.services.event.as_ref().unwrap();
        let event_url               = url::Url::parse(&url)?;
        _      = Camera::pull_messages(event_url, auth.as_ref()).await?;

        Ok(())
    }
}

#[rustfmt::skip]
impl Camera {
    pub fn new(base: Device) -> Self {
        Camera {
            auth: base.auth.clone(),
            base,
            capabilities:         Capabilities::default(),
            profiles:             Profiles::default(),
            device_info:          DeviceInfo::default(),
            stream:               StreamUri::default(),
            services:             Services::default(),
            event_props:          EventCapabilities::default(),
            analytics_props:      AnalyticsCapabilities::default(),
            analytics_configs:    AnalyticsConfigList::default(),
        }
    }
}

#[rustfmt::skip]
impl From<&str> for Camera {
    fn from(input: &str) -> Self {
        let url_onvif = match url::Url::parse(input) {
            Ok(url) => url,
            Err(e) => panic!("[Device][Camera] Error parsing str: {e}"),
        };

        let base = Device {
            url_onvif,
            device_type:    DeviceTypes::Camera,
            scopes:         Vec::new(),
            auth:           None,
        };    

        Camera {
            base,
            auth:           None,
            capabilities:         Capabilities::default(),
            profiles:             Profiles::default(),
            device_info:          DeviceInfo::default(),
            stream:               StreamUri::default(),
            services:             Services::default(),
            event_props:          EventCapabilities::default(),
            analytics_props:      AnalyticsCapabilities::default(),
            analytics_configs:    AnalyticsConfigList::default(),
        }
    }
}
