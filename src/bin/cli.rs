use embedded_hal_async::delay::DelayNs;
//use embedded_io_adapters::std::FromStd;
use embedded_io_adapters::tokio_1::FromTokio;
use inquire::Select;
use sds011::{Config, SDS011};
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
    let ports = tokio_serial::available_ports().expect("No ports found!");
    let ports: Vec<&String> = ports.iter().map(|p| &p.port_name).collect();
    let ans = Select::new("Which serial port should be used?", ports).prompt()?;

    let builder = tokio_serial::new(ans, 9600).timeout(Duration::from_secs(1));
    let serial = SerialStream::open(&builder)?;

    let mut adapter = FromTokio::new(serial);
    let sensor = SDS011::new(&mut adapter, Config::default());

    // initialize (puts the sensor into Polling state)
    let mut sensor = sensor.init(&mut Delay).await?;
    let fw = sensor.version();
    let id = sensor.id();
    println!("SDS011, ID: {id}, Firmware: {fw}");

    // read the sensor once
    let vals = sensor.measure(&mut Delay).await?;
    println!("{vals}");

    // now, put the sensor into periodic state (reports every 5 minutes)
    let mut sensor = sensor.make_periodic(5).await?;

    loop {
        let res = sensor.measure().await?;
        println!("{res}");
    }
}
