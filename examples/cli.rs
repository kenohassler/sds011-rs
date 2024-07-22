use embedded_hal::delay::DelayNs;
use embedded_io_adapters::std::FromStd;
use inquire::Select;
use sds011::{Config, SDS011};
use serialport;
use std::env;
use std::error::Error;
use std::thread::sleep;
use std::time::Duration;

struct Delay;

impl DelayNs for Delay {
    fn delay_ns(&mut self, n: u32) {
        sleep(Duration::from_nanos(n.into()));
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args: Vec<String> = env::args().collect();

    // use the first arg as serial port, query interactively if not given
    let port;
    if args.len() == 2 {
        port = args.pop().unwrap();
    } else {
        let ports = tokio_serial::available_ports()?;
        let ports: Vec<String> = ports.into_iter().map(|p| p.port_name).collect();
        port = Select::new("Which serial port should be used?", ports).prompt()?
    }

    let builder = serialport::new(port, 9600).timeout(Duration::from_secs(1));
    let serial = builder.open()?;

    let mut adapter = FromStd::new(serial);
    let sensor = SDS011::new(&mut adapter, Config::default());

    // initialize (puts the sensor into Polling state)
    let mut sensor = sensor.init(&mut Delay)?;
    let fw = sensor.version();
    let id = sensor.id();
    println!("SDS011, ID: {id}, Firmware: {fw}");

    let vals = sensor.measure(&mut Delay)?;
    println!("{vals}");

    Ok(())
}
