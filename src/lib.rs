//! This crate implements a driver for the SDS011 particle sensor based on
//! [`embedded-hal`](https://github.com/rust-embedded/embedded-hal).
//! Thanks to this abstraction layer, it can be used on full-fledged operating
//! systems as well as embedded devices.
//!
//! # Features
//! * `sync`: To use the synchronous interface, enable this feature.
//!   By default, this library exposes an async API.
//!
//! # Examples
//! The crate ships with two small CLI examples that utilize the library:
//! * [`cli.rs`](examples/cli.rs) uses the synchronous interface (embedded-io),
//! * [`cli_async.rs`](examples/cli_async.rs) uses the asynchronous interface
//!   (embedded-io-async).
//!
//! The example below demonstrates how to use the sensor with an ESP32,
//! showcasing the strength of the embedded-hal abstractions.
//!
//! ```ignore
//! #![no_std]
//! #![no_main]
//!
//! use embassy_executor::Spawner;
//! use embassy_time::{Duration, Timer, Delay};
//! use esp_backtrace as _;
//! use esp_hal::{
//!     clock::ClockControl,
//!     gpio::Io,
//!     peripherals::Peripherals,
//!     prelude::*,
//!     system::SystemControl,
//!     timer::timg::TimerGroup,
//!     uart::{config::Config, TxRxPins, Uart},
//! };
//! use esp_println::println;
//! use sds011::SDS011;
//!
//! #[main]
//! async fn main(_s: Spawner) -> ! {
//!     let peripherals = Peripherals::take();
//!     let system = SystemControl::new(peripherals.SYSTEM);
//!     let clocks = ClockControl::max(system.clock_control).freeze();
//!
//!     let timg0 = TimerGroup::new(peripherals.TIMG0, &clocks);
//!     esp_hal_embassy::init(&clocks, timg0.timer0);
//!
//!     let io = Io::new(peripherals.GPIO, peripherals.IO_MUX);
//!     let (tx_pin, rx_pin) = (io.pins.gpio3, io.pins.gpio2);
//!     let config = Config::default()
//!         .baudrate(9600)
//!         .rx_fifo_full_threshold(10);
//!
//!     let mut uart1 =
//!         Uart::new_async_with_config(peripherals.UART1, config, &clocks, tx_pin, rx_pin).unwrap();
//!
//!     let sds011 = SDS011::new(&mut uart1, sds011::Config::default());
//!     let mut sds011 = sds011.init(&mut Delay).await.unwrap();
//!
//!     println!("SDS011 version {}, ID {}", sds011.version(), sds011.id());
//!     loop {
//!         let dust = sds011.measure(&mut Delay).await.unwrap();
//!         println!("{}", dust);
//!
//!         Timer::after(Duration::from_millis(30_000)).await;
//!     }
//! }
//! ```
//!
//! # Technical Overview
//! The sensor has two operating modes:
//! * "query mode": The sensor does nothing until it is actively instructed to
//!   perform a measurement (we call this polling).
//! * "active mode": The sensor continuously produces data in a configurable
//!   interval (we call this periodic).
//!
//! We abstract this into the following interface:
//! * A sensor created using `new()` is in `Uninitialized` state.
//!   No serial communication is performed during creation.
//! * You call `init()`. This will return a sensor in `Polling` state.
//!   The sensor is instructed via serial commands to switch to query mode and
//!   goes to sleep (fan off).
//! * The sensor can now be queried via the `measure()` function.
//!   This will wake the sensor, spin the fan for a configurable duration
//!   (which is necessary to get a correct measurement), read the sensor and
//!   put it back to sleep.
//! * Optionally (not recommended!), the sensor can be put into `Periodic` state
//!   by calling `make_periodic()` on a sensor in `Polling` state.
//!   This puts the sensor in charge of sleeping and waking up.
//!   Since it will continuously produce data, make sure to call `measure()`
//!   in time so the serial output buffer does not overflow.
//!
//! # Limitations
//! This abstraction does not yet support sending commands only to a specific
//! sensor id (it effectively uses broadcast mode all the time).
//! This feature seemed irrelevant, but the backend code for it is completely
//! implemented, so this may change in a future version if there is demand.
//! Also, putting sensors into periodic mode can have the side effect of missing
//! package boundaries. The current version cannot recover from this; it will
//! return an error. Close the serial port and retry, or probably better,
//! just don't use periodic mode.
//!
//! # Acknowledgements
//! Thank you to Tim Orme, who implemented sds011lib in Python
//! and wrote [documentation](https://timorme.github.io/sds011lib/resource/)
//! that pointed me in the right direction, especially to:
//! * [The Data Sheet](https://cdn-reichelt.de/documents/datenblatt/X200/SDS011-DATASHEET.pdf)
//! * [The Control Protocol](https://cdn.sparkfun.com/assets/parts/1/2/2/7/5/Laser_Dust_Sensor_Control_Protocol_V1.3.pdf)
//!
//! for the SDS011 sensor.

#![no_std]
#![warn(clippy::pedantic)]
#![warn(clippy::cargo)]

use core::fmt::Debug;
use core::marker::PhantomData;
#[cfg(feature = "sync")]
use embedded_hal::delay::DelayNs;
#[cfg(not(feature = "sync"))]
use embedded_hal_async::delay::DelayNs;
#[cfg(feature = "sync")]
use embedded_io::{Read, ReadExactError, Write};
#[cfg(not(feature = "sync"))]
use embedded_io_async::{Read, ReadExactError, Write};
use maybe_async::maybe_async;
pub use message::{FirmwareVersion, Measurement};
use message::{
    Kind, Message, ParseError, RECV_BUF_SIZE, Reporting, ReportingMode, Sleep, SleepMode,
    WorkingPeriod,
};
use thiserror::Error;

mod message;

/// Sensor configuration, specifically delay times.
///
/// Delays are necessary between waking up the sensor
/// and reading its value to stabilize the measurement.
#[derive(Debug, Clone)]
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
    #[must_use]
    pub fn set_measure_delay(mut self, measure_delay: u32) -> Self {
        self.measure_delay = measure_delay;
        self
    }

    /// How many milliseconds to wait before waking the sensor; defaults to 500.
    /// Setting this too low can result in the sensor not coming up (boot time?)
    #[must_use]
    pub fn set_sleep_delay(mut self, sleep_delay: u32) -> Self {
        self.sleep_delay = sleep_delay;
        self
    }
}

/// Error type for operations on the SDS011 sensor.
#[derive(Debug, Error)]
pub enum SDS011Error<E> {
    /// A received message could not be decoded.
    #[error("message could not be decoded: {0}")]
    ParseError(#[from] ParseError),
    /// The serial interface returned an error while reading.
    #[error("serial read error: {0}")]
    ReadError(E),
    /// The serial interface returned an error while writing.
    #[error("serial write error: {0}")]
    WriteError(E),
    /// Encountered an EOF while reading.
    #[error("unexpected EOF")]
    UnexpectedEof,
    /// The received message was not expected in the current sensor state.
    #[error("unexpected message type")]
    UnexpectedType,
    /// The requested operation failed.
    #[error("requested operation failed")]
    OperationFailed,
    /// The given parameters were invalid.
    #[error("given parameters were invalid")]
    Invalid,
}

pub mod sensor_state {
    mod private {
        pub trait Sealed {}
    }

    /// Encodes state for the [SDS011](crate::SDS011) struct,
    /// as explained in the [technical overview](crate#technical-overview).
    ///
    /// This trait is sealed to prevent external implementations.
    pub trait SensorState: private::Sealed {}

    /// Sensor reports periodically
    pub struct Periodic;
    impl private::Sealed for Periodic {}
    impl SensorState for Periodic {}

    /// Sensor sleeps until polled
    pub struct Polling;
    impl private::Sealed for Polling {}
    impl SensorState for Polling {}

    /// Sensor not yet initialized
    pub struct Uninitialized;
    impl private::Sealed for Uninitialized {}
    impl SensorState for Uninitialized {}
}

pub use sensor_state::SensorState;
use sensor_state::{Periodic, Polling, Uninitialized};

/// The main struct.
/// Wraps around a serial interface that implements embedded-io(-async).
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
    #[maybe_async]
    async fn get_reply(&mut self) -> Result<Message, SDS011Error<RW::Error>> {
        let mut buf = [0u8; RECV_BUF_SIZE];

        match self.serial.read_exact(&mut buf).await {
            Ok(()) => Ok(Message::parse_reply(&buf)?),
            Err(ReadExactError::UnexpectedEof) => Err(SDS011Error::UnexpectedEof),
            Err(ReadExactError::Other(e)) => Err(SDS011Error::ReadError(e)),
        }
    }

    #[maybe_async]
    async fn send_message(&mut self, kind: Kind) -> Result<(), SDS011Error<RW::Error>> {
        let msg = Message::new(kind, self.sensor_id);
        let out_buf = msg.create_query();

        self.serial
            .write_all(&out_buf)
            .await
            .map_err(SDS011Error::WriteError)
    }

    #[maybe_async]
    async fn read_sensor(&mut self, query: bool) -> Result<Measurement, SDS011Error<RW::Error>> {
        if query {
            self.send_message(Kind::Query(None)).await?;
        }

        match self.get_reply().await?.kind {
            Kind::Query(data) => Ok(data.expect("replies always contain data")),
            _ => Err(SDS011Error::UnexpectedType),
        }
    }

    #[maybe_async]
    async fn get_firmware(&mut self) -> Result<(u16, FirmwareVersion), SDS011Error<RW::Error>> {
        self.send_message(Kind::FWVersion(None)).await?;

        let reply = self.get_reply().await?;
        let id = reply.sensor_id.expect("replies always contain data");
        match reply.kind {
            Kind::FWVersion(data) => Ok((id, data.expect("replies always contain data"))),
            _ => Err(SDS011Error::UnexpectedType),
        }
    }

    #[maybe_async]
    async fn _get_runmode(&mut self) -> Result<ReportingMode, SDS011Error<RW::Error>> {
        let r = Reporting::new_query();
        self.send_message(Kind::ReportingMode(r)).await?;

        match self.get_reply().await?.kind {
            Kind::ReportingMode(data) => Ok(data.mode()),
            _ => Err(SDS011Error::UnexpectedType),
        }
    }

    #[maybe_async]
    async fn set_runmode_query(&mut self) -> Result<(), SDS011Error<RW::Error>> {
        let r = Reporting::new_set(ReportingMode::Query);
        self.send_message(Kind::ReportingMode(r)).await?;

        match self.get_reply().await?.kind {
            Kind::ReportingMode(r) => match r.mode() {
                ReportingMode::Query => Ok(()),
                ReportingMode::Active => Err(SDS011Error::OperationFailed),
            },
            _ => Err(SDS011Error::UnexpectedType),
        }
    }

    #[maybe_async]
    async fn set_runmode_active(&mut self) -> Result<(), SDS011Error<RW::Error>> {
        let r = Reporting::new_set(ReportingMode::Active);
        self.send_message(Kind::ReportingMode(r)).await?;

        match self.get_reply().await?.kind {
            Kind::ReportingMode(r) => match r.mode() {
                ReportingMode::Active => Ok(()),
                ReportingMode::Query => Err(SDS011Error::OperationFailed),
            },
            _ => Err(SDS011Error::UnexpectedType),
        }
    }

    #[maybe_async]
    async fn _get_period(&mut self) -> Result<u8, SDS011Error<RW::Error>> {
        let w = WorkingPeriod::new_query();
        self.send_message(Kind::WorkingPeriod(w)).await?;

        match self.get_reply().await?.kind {
            Kind::WorkingPeriod(data) => Ok(data.period()),
            _ => Err(SDS011Error::UnexpectedType),
        }
    }

    #[maybe_async]
    async fn set_period(&mut self, minutes: u8) -> Result<(), SDS011Error<RW::Error>> {
        let w = WorkingPeriod::new_set(minutes);
        self.send_message(Kind::WorkingPeriod(w)).await?;

        match self.get_reply().await?.kind {
            Kind::WorkingPeriod(data) if data.period() == minutes => Ok(()),
            Kind::WorkingPeriod(_) => Err(SDS011Error::OperationFailed),
            _ => Err(SDS011Error::UnexpectedType),
        }
    }

    #[maybe_async]
    async fn _get_sleep(&mut self) -> Result<SleepMode, SDS011Error<RW::Error>> {
        let s = Sleep::new_query();
        self.send_message(Kind::Sleep(s)).await?;

        match self.get_reply().await?.kind {
            Kind::Sleep(data) => Ok(data.sleep_mode()),
            _ => Err(SDS011Error::UnexpectedType),
        }
    }

    #[maybe_async]
    async fn sleep(&mut self) -> Result<(), SDS011Error<RW::Error>> {
        let s = Sleep::new_set(SleepMode::Sleep);
        self.send_message(Kind::Sleep(s)).await?;

        match self.get_reply().await?.kind {
            Kind::Sleep(s) => match s.sleep_mode() {
                SleepMode::Sleep => Ok(()),
                SleepMode::Work => Err(SDS011Error::OperationFailed),
            },
            _ => Err(SDS011Error::UnexpectedType),
        }
    }

    #[maybe_async]
    async fn wake(&mut self) -> Result<(), SDS011Error<RW::Error>> {
        let s = Sleep::new_set(SleepMode::Work);
        self.send_message(Kind::Sleep(s)).await?;

        match self.get_reply().await?.kind {
            Kind::Sleep(s) => match s.sleep_mode() {
                SleepMode::Work => Ok(()),
                SleepMode::Sleep => Err(SDS011Error::OperationFailed),
            },
            _ => Err(SDS011Error::UnexpectedType),
        }
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
    ///
    /// # Errors
    /// This communicates with the sensor over serial and may fail with any
    /// [SDS011Error].
    #[maybe_async]
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
    ///
    /// # Errors
    /// This communicates with the sensor over serial and may fail with any
    /// [SDS011Error].
    #[maybe_async]
    pub async fn measure(&mut self) -> Result<Measurement, SDS011Error<RW::Error>> {
        self.read_sensor(false).await
    }

    /// Get the sensor's ID.
    #[allow(clippy::missing_panics_doc)] // should never panic
    pub fn id(&self) -> u16 {
        self.sensor_id.expect("sensor is initialized")
    }

    /// Get the sensor's firmware version.
    #[allow(clippy::missing_panics_doc)] // should never panic
    pub fn version(&self) -> FirmwareVersion {
        self.firmware.clone().expect("sensor is initialized")
    }
}

impl<RW> SDS011<RW, Polling>
where
    RW: Read + Write,
{
    /// In this state, measurements are triggered by calling this function.
    /// The sensor is woken up and the fan spins for the configured delay time,
    /// after which we send the measurement query and put it back to sleep.
    ///
    /// # Errors
    /// This communicates with the sensor over serial and may fail with any
    /// [SDS011Error].
    #[maybe_async]
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
    ///
    /// # Errors
    /// This communicates with the sensor over serial and may fail with any
    /// [SDS011Error].
    #[maybe_async]
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

    /// Get the sensor's ID.
    #[allow(clippy::missing_panics_doc)] // should never panic
    pub fn id(&self) -> u16 {
        self.sensor_id.expect("sensor is initialized")
    }

    /// Get the sensor's firmware version.
    #[allow(clippy::missing_panics_doc)] // should never panic
    pub fn version(&self) -> FirmwareVersion {
        self.firmware.clone().expect("sensor is initialized")
    }
}
