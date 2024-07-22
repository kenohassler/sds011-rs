use embedded_hal_async::delay::DelayNs;
use embedded_io_adapters::tokio_1::FromTokio;
use inquire::Select;
use sds011::{Config, SDS011};
use std::env;
use std::error::Error;
use std::time::Duration;
use tokio::time::sleep;
use tokio_serial::SerialStream;

struct Delay;

impl DelayNs for Delay {
    async fn delay_ns(&mut self, n: u32) {
        sleep(Duration::from_nanos(n.into())).await;
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut args: Vec<String> = env::args().collect();

    // use the first arg as serial port, query interactively if not given
    let port = if args.len() == 2 {
        args.pop().unwrap()
    } else {
        let ports = tokio_serial::available_ports()?;
        let ports: Vec<String> = ports.into_iter().map(|p| p.port_name).collect();
        Select::new("Which serial port should be used?", ports).prompt()?
    };

    let builder = tokio_serial::new(port, 9600).timeout(Duration::from_secs(1));
    let serial = SerialStream::open(&builder)?;

    let mut adapter = FromTokio::new(serial);
    let sensor = SDS011::new(&mut adapter, Config::default());

    // initialize (puts the sensor into Polling state)
    let mut sensor = sensor.init(&mut Delay).await?;
    let fw = sensor.version();
    let id = sensor.id();
    println!("SDS011, ID: {id}, Firmware: {fw}");

    let vals = sensor.measure(&mut Delay).await?;
    println!("{vals}");

    Ok(())
}
