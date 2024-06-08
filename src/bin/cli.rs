//use embedded_io_adapters::std::FromStd;
use embedded_io_adapters::tokio_1::FromTokio;
use futures::executor::block_on;
use inquire::Select;
use sds011::SDS011;
use std::error::Error;
use std::thread::sleep;
use std::time::Duration;
use tokio_serial::SerialStream;

fn main() -> Result<(), Box<dyn Error>> {
    let ports = tokio_serial::available_ports().expect("No ports found!");
    let ports: Vec<&String> = ports.iter().map(|p| &p.port_name).collect();
    let ans = Select::new("Which serial port should be used?", ports).prompt()?;

    let builder = tokio_serial::new(ans, 9600).timeout(Duration::from_secs(1));
    let serial = SerialStream::open(&builder)?;

    let mut adapter = FromTokio::new(serial);
    let mut sensor = SDS011::new(&mut adapter);

    // sensor.set_sleep();

    // let sleep = sensor.get_sleep();
    // println!("sleep status: {sleep:?}");

    block_on(sensor.set_work())?;

    sleep(Duration::from_secs(10));

    let vals = block_on(sensor.read_sensor_active())?;
    println!(
        "PM2.5: {} µg/m3 \t PM10: {} µg/m3",
        vals.pm25(),
        vals.pm10()
    );

    // _ = sensor.set_query_mode();

    let fw = block_on(sensor.get_firmware())?;
    println!("FW version: {fw}");

    let rep_md = block_on(sensor.get_runmode())?;
    println!("reporting mode: {rep_md:?}");

    let period = block_on(sensor.get_period())?;
    println!("measuring period: {period} mins");

    let sleep = block_on(sensor.get_sleep())?;
    println!("sleep status: {sleep:?}");

    block_on(sensor.set_sleep())?;

    //sensor.set_query_mode();

    Ok(())
}
