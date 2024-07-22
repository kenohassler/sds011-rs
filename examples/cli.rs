use clap::Parser;
use embedded_hal::delay::DelayNs;
use embedded_io_adapters::std::FromStd;
use inquire::Select;
use sds011::{Config, SDS011};
use serialport;
use std::error::Error;
use std::thread::sleep;
use std::time::Duration;

struct Delay;

impl DelayNs for Delay {
    fn delay_ns(&mut self, n: u32) {
        sleep(Duration::from_nanos(n.into()));
    }
}

/// Simple CLI to poll the SDS011 fine particle sensor
#[derive(Parser, Debug)]
#[command(version)]
struct Args {
    /// Which serial port to use. Will be queried interactively if not given.
    port: Option<String>,
    /// Poll the sensor every n minutes, 0 for one-shot.
    #[arg(short = 'n', long, default_value_t = 0)]
    interval: u32,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let ans = match args.port {
        Some(p) => p,
        None => {
            let ports = serialport::available_ports().expect("No ports found!");
            let ports: Vec<String> = ports.into_iter().map(|p| p.port_name).collect();
            Select::new("Which serial port should be used?", ports).prompt()?
        }
    };

    let builder = serialport::new(ans, 9600).timeout(Duration::from_secs(1));
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

    // continuously measure every n minutes (taking 30s measurement delay into account)
    if args.interval != 0 {
        loop {
            Delay.delay_ms((args.interval * 60 - 30) * 1000);

            let vals = sensor.measure(&mut Delay)?;
            println!("{vals}");
        }
    }

    Ok(())
}
