use anyhow::Result;
use onvif_cam_rs::builder::camera::CameraBuilder;
use onvif_cam_rs::device::camera::Camera;
use onvif_cam_rs::device::Auth;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    println!("----------------------- DEVICE DISCOVERY ----------------------");

    // let mut devices = client::discover().await?;
    // let mut cameras: Vec<Camera> = Vec::new();

    // for device in devices {
    //     let mut camera = Camera::new(device);
    //     camera.build_all().await?;
    //     cameras.push(camera);
    // }

    // Without authentication (if camera allows anonymous access)
    // let mut camera = Camera::from("http://192.168.86.200:8080/onvif/device_service");

    // With WS-UsernameToken authentication
    let mut camera = Camera::from("http://192.168.86.200:8080/onvif/device_service");
    camera.auth = Some(Auth::new("admin", "password"));
    camera.build_all().await?;

    // Print stream URI if available
    if let Some(uri) = &camera.stream.uri {
        println!("RTSP Stream URI: {uri}");
    }

    Ok(())
}
