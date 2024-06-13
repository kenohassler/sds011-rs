#![no_std]
#![feature(error_in_core)]

//use embedded_io::{Read, Write};
use crate::message::ParseError;
use core::error::Error;
use core::fmt::{Debug, Display, Formatter};
use core::marker::PhantomData;
use embedded_hal_async::delay::DelayNs;
use embedded_io_async::{Read, Write};
pub use message::FirmwareVersion;
pub use message::Measurement;
use message::Message;
use message::MessageType;
use message::Reporting;
use message::ReportingMode;
use message::Sleep;
use message::SleepMode;
use message::WorkingPeriod;

mod message;

/// The expected receive message length.
///
/// This is needed for buffer configuration in some UART implementations,
/// else `read()` calls block forever waiting for more data.
pub const READ_BUF_SIZE: usize = 10;

/// Sensor configuration, specifically delay times.
///
/// Delays are necessary between waking up the sensor
/// and reading its value to stabilize the measurement.
pub struct Config {
    sleep_delay: u32,
    measure_delay: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            sleep_delay: 500,
            measure_delay: 30_000,
        }
    }
}

impl Config {
    /// Configure the time between waking the sensor (spinning up the fan)
    /// and reading the measurement, in milliseconds.
    /// The sensor manual recommends 30 seconds, which is the default.
    pub fn set_measure_delay(mut self, measure_delay: u32) -> Self {
        self.measure_delay = measure_delay;
        self
    }

    /// How many milliseconds to wait before waking the sensor; defaults to 500.
    /// Setting this too low can result in the sensor not coming up (boot time?)
    pub fn set_sleep_delay(mut self, sleep_delay: u32) -> Self {
        self.sleep_delay = sleep_delay;
        self
    }
}

/// Error type for operations on the SDS011 sensor.
pub enum SDS011Error<E> {
    /// The received message could not be decoded.
    ParseError(ParseError),
    /// The serial interface returned an error while reading.
    ReadError(E),
    /// The serial interface returned an error while writing.
    WriteError(E),
    /// We received a message shorter than the fixed message length (10 bytes).
    ShortRead,
    /// We're unable to send the full message (19 bytes) at once.
    ShortWrite,
    /// The received message was not expected in the current sensor state.
    UnexpectedType,
    /// The requested operation failed.
    OperationFailed,
    /// The given parameters were invalid.
    Invalid,
}

impl<E> Display for SDS011Error<E> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            SDS011Error::ParseError(e) => f.write_fmt(format_args!("ParseError: {e}")),
            SDS011Error::ReadError(_) => f.write_str("Serial read error"),
            SDS011Error::WriteError(_) => f.write_str("Serial write error"),
            SDS011Error::ShortRead => f.write_str("Short read"),
            SDS011Error::ShortWrite => f.write_str("Short write"),
            SDS011Error::UnexpectedType => f.write_str("Unexpected message type"),
            SDS011Error::OperationFailed => f.write_str("The requested operation failed"),
            SDS011Error::Invalid => f.write_str("The given parameters were invalid"),
        }
    }
}

impl<E> Debug for SDS011Error<E> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        Display::fmt(self, f)
    }
}

impl<E> Error for SDS011Error<E> {}

mod sensor_trait {
    pub trait SensorState {}

    pub struct Periodic;
    impl SensorState for Periodic {}

    pub struct Polling;
    impl SensorState for Polling {}

    pub struct Uninitialized;
    impl SensorState for Uninitialized {}
}

use sensor_trait::SensorState;
use sensor_trait::{Periodic, Polling, Uninitialized};

/// The main struct.
/// Wraps around a serial interface that implements embedded-io-async.
///
/// Calling `new()` will give you an uninitialized struct.
/// You need to call `init()` on it to get a sensor that can be polled.
pub struct SDS011<RW, S: SensorState> {
    serial: RW,
    config: Config,
    sensor_id: Option<u16>,
    firmware: Option<FirmwareVersion>,
    _state: PhantomData<S>,
}

impl<RW, S> SDS011<RW, S>
where
    RW: Read + Write,
    S: SensorState,
{
    async fn get_reply(&mut self) -> Result<Message, SDS011Error<RW::Error>> {
        let mut buf = [0u8; READ_BUF_SIZE];

        match self.serial.read(&mut buf).await {
            Ok(n) if n == buf.len() => match Message::parse_reply(&buf) {
                Ok(m) => Ok(m),
                Err(e) => Err(SDS011Error::ParseError(e)),
            },
            Ok(_) => Err(SDS011Error::ShortRead),
            Err(e) => Err(SDS011Error::ReadError(e)),
        }
    }

    async fn send_message(&mut self, m_type: MessageType) -> Result<(), SDS011Error<RW::Error>> {
        let msg = Message::new(m_type, self.sensor_id);
        let out_buf = msg.create_query();

        match self.serial.write(&out_buf).await {
            Ok(n) if n == out_buf.len() => Ok(()),
            Ok(_) => Err(SDS011Error::ShortWrite),
            Err(e) => Err(SDS011Error::WriteError(e)),
        }
    }

    async fn read_sensor(&mut self, query: bool) -> Result<Measurement, SDS011Error<RW::Error>> {
        if query {
            self.send_message(MessageType::Query(None)).await?;
        }

        match self.get_reply().await?.m_type {
            MessageType::Query(data) => Ok(data.expect("replies always contain data")),
            _ => Err(SDS011Error::UnexpectedType),
        }
    }

    async fn get_firmware(&mut self) -> Result<(u16, FirmwareVersion), SDS011Error<RW::Error>> {
        self.send_message(MessageType::FWVersion(None)).await?;

        let reply = self.get_reply().await?;
        let id = reply.sensor_id.expect("replies always contain data");
        match reply.m_type {
            MessageType::FWVersion(data) => Ok((id, data.expect("replies always contain data"))),
            _ => Err(SDS011Error::UnexpectedType),
        }
    }

    async fn _get_runmode(&mut self) -> Result<ReportingMode, SDS011Error<RW::Error>> {
        let r = Reporting::new_query();
        self.send_message(MessageType::ReportingMode(r)).await?;

        match self.get_reply().await?.m_type {
            MessageType::ReportingMode(data) => Ok(data.mode()),
            _ => Err(SDS011Error::UnexpectedType),
        }
    }

    async fn set_runmode_query(&mut self) -> Result<(), SDS011Error<RW::Error>> {
        let r = Reporting::new_set(ReportingMode::Query);
        self.send_message(MessageType::ReportingMode(r)).await?;

        match self.get_reply().await?.m_type {
            MessageType::ReportingMode(r) => match r.mode() {
                ReportingMode::Query => Ok(()),
                _ => Err(SDS011Error::OperationFailed),
            },
            _ => Err(SDS011Error::UnexpectedType),
        }
    }

    async fn set_runmode_active(&mut self) -> Result<(), SDS011Error<RW::Error>> {
        let r = Reporting::new_set(ReportingMode::Active);
        self.send_message(MessageType::ReportingMode(r)).await?;

        match self.get_reply().await?.m_type {
            MessageType::ReportingMode(r) => match r.mode() {
                ReportingMode::Active => Ok(()),
                _ => Err(SDS011Error::OperationFailed),
            },
            _ => Err(SDS011Error::UnexpectedType),
        }
    }

    async fn _get_period(&mut self) -> Result<u8, SDS011Error<RW::Error>> {
        let w = WorkingPeriod::new_query();
        self.send_message(MessageType::WorkingPeriod(w)).await?;

        match self.get_reply().await?.m_type {
            MessageType::WorkingPeriod(data) => Ok(data.period()),
            _ => Err(SDS011Error::UnexpectedType),
        }
    }

    async fn set_period(&mut self, minutes: u8) -> Result<(), SDS011Error<RW::Error>> {
        let w = WorkingPeriod::new_set(minutes);
        self.send_message(MessageType::WorkingPeriod(w)).await?;

        match self.get_reply().await?.m_type {
            MessageType::WorkingPeriod(data) if data.period() == minutes => Ok(()),
            MessageType::WorkingPeriod(_) => Err(SDS011Error::OperationFailed),
            _ => Err(SDS011Error::UnexpectedType),
        }
    }

    async fn _get_sleep(&mut self) -> Result<SleepMode, SDS011Error<RW::Error>> {
        let s = Sleep::new_query();
        self.send_message(MessageType::Sleep(s)).await?;

        match self.get_reply().await?.m_type {
            MessageType::Sleep(data) => Ok(data.sleep_mode()),
            _ => Err(SDS011Error::UnexpectedType),
        }
    }

    async fn sleep(&mut self) -> Result<(), SDS011Error<RW::Error>> {
        let s = Sleep::new_set(SleepMode::Sleep);
        self.send_message(MessageType::Sleep(s)).await?;

        // quirky response (FF instead of AB byte)
        match self.get_reply().await?.m_type {
            MessageType::Sleep(s) => match s.sleep_mode() {
                SleepMode::Sleep => Ok(()),
                _ => Err(SDS011Error::OperationFailed),
            },
            _ => Err(SDS011Error::UnexpectedType),
        }
    }

    async fn wake(&mut self) -> Result<(), SDS011Error<RW::Error>> {
        let s = Sleep::new_set(SleepMode::Work);
        self.send_message(MessageType::Sleep(s)).await?;

        match self.get_reply().await?.m_type {
            MessageType::Sleep(s) => match s.sleep_mode() {
                SleepMode::Work => Ok(()),
                _ => Err(SDS011Error::OperationFailed),
            },
            _ => Err(SDS011Error::UnexpectedType),
        }
    }

    /// Get the sensor's ID (**panics** if sensor is uninitialized).
    pub fn id(&self) -> u16 {
        self.sensor_id.expect("sensor uninitialized")
    }

    /// Get the sensor's firmware version (**panics** if sensor is uninitialized).
    pub fn version(&self) -> FirmwareVersion {
        self.firmware.clone().expect("sensor uninitialized")
    }
}

impl<RW> SDS011<RW, Uninitialized>
where
    RW: Read + Write,
{
    /// Create a new sensor instance, consuming the serial interface.
    /// The returned instance needs to be initialized before use.
    pub fn new(serial: RW, config: Config) -> Self {
        SDS011::<RW, Uninitialized> {
            serial,
            config,
            sensor_id: None,
            firmware: None,
            _state: PhantomData,
        }
    }

    /// Put the sensor in a well-defined state (sleeping in polling mode).
    pub async fn init<D: DelayNs>(
        mut self,
        delay: &mut D,
    ) -> Result<SDS011<RW, Polling>, SDS011Error<RW::Error>> {
        // sleep a short moment to make sure the sensor is ready
        delay.delay_ms(self.config.sleep_delay).await;
        self.wake().await?;

        self.set_runmode_query().await?;

        // while we're at it, read the firmware version once
        let (id, firmware) = self.get_firmware().await?;
        self.sleep().await?;

        Ok(SDS011::<RW, Polling> {
            serial: self.serial,
            config: self.config,
            sensor_id: Some(id),
            firmware: Some(firmware),
            _state: PhantomData,
        })
    }
}

impl<RW> SDS011<RW, Periodic>
where
    RW: Read + Write,
{
    /// In this state, the sensor will wake up periodically (as configured),
    /// wait 30 seconds, send a measurement over serial, and go back to sleep.
    /// This method waits until data is available before returning.
    pub async fn measure(&mut self) -> Result<Measurement, SDS011Error<RW::Error>> {
        self.read_sensor(false).await
    }
}

impl<RW> SDS011<RW, Polling>
where
    RW: Read + Write,
{
    /// In this state, measurements are triggered by calling this function.
    /// The sensor is woken up and the fan spins for the configured delay time,
    /// after which we send the measurement query and put it back to sleep.
    pub async fn measure<D: DelayNs>(
        &mut self,
        delay: &mut D,
    ) -> Result<Measurement, SDS011Error<RW::Error>> {
        // sleep a short moment to make sure the sensor is ready
        delay.delay_ms(self.config.sleep_delay).await;
        self.wake().await?;

        // do a dummy measurement, spin for a few secs, then do real measurement
        _ = self.read_sensor(true).await?;
        delay.delay_ms(self.config.measure_delay).await;
        let res = self.read_sensor(true).await?;
        self.sleep().await?;

        Ok(res)
    }

    /// Set the sensor into periodic measurement mode, in which it performs
    /// a measurement every 0-30 `minutes`.
    /// If > 0, the sensor will go to sleep between measurements.
    pub async fn make_periodic<D: DelayNs>(
        mut self,
        delay: &mut D,
        minutes: u8,
    ) -> Result<SDS011<RW, Periodic>, SDS011Error<RW::Error>> {
        if minutes > 30 {
            return Err(SDS011Error::Invalid);
        }

        // sleep a short moment to make sure the sensor is ready
        delay.delay_ms(self.config.sleep_delay).await;
        self.wake().await?;

        self.set_period(minutes).await?;
        self.set_runmode_active().await?;

        Ok(SDS011::<RW, Periodic> {
            serial: self.serial,
            config: self.config,
            sensor_id: self.sensor_id,
            firmware: self.firmware,
            _state: PhantomData,
        })
    }
}

#[cfg(test)]
mod tests {}
