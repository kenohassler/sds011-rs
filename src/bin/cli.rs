use embedded_io_adapters::std::FromStd;
//use embedded_io_adapters::tokio_1::FromTokio;
use inquire::Select;
use sds011::SDS011;
use std::error::Error;
use std::thread::sleep;
use std::time::Duration;

fn main() -> Result<(), Box<dyn Error>> {
    let ports = tokio_serial::available_ports().expect("No ports found!");
    let ports: Vec<&String> = ports.iter().map(|p| &p.port_name).collect();
    let ans = Select::new("Which serial port should be used?", ports).prompt()?;

    let serial = tokio_serial::new(ans, 9600)
        .timeout(Duration::from_secs(1))
        .open()?;
    let mut adapter = FromStd::new(serial);
    let mut sensor = SDS011::new(&mut adapter);

    // sensor.set_sleep();

    // let sleep = sensor.get_sleep();
    // println!("sleep status: {sleep:?}");

    sensor.set_work();

    sleep(Duration::from_secs(10));

    let vals = sensor.read_sensor_active();
    println!(
        "PM2.5: {} µg/m3 \t PM10: {} µg/m3",
        vals.pm25(),
        vals.pm10()
    );

    // _ = sensor.set_query_mode();

    let fw = sensor.get_firmware();
    println!("FW version: {fw}");

    let rep_md = sensor.get_runmode();
    println!("reporting mode: {rep_md:?}");

    let period = sensor.get_period();
    println!("measuring period: {period} mins");

    let sleep = sensor.get_sleep();
    println!("sleep status: {sleep:?}");

    sensor.set_sleep();

    //sensor.set_query_mode();

    Ok(())
}
