#![no_std]

//use embedded_io::{Read, Write};
use crate::message::ParseError;
use core::error::Error;
use core::fmt::{Debug, Display, Formatter};
use core::marker::PhantomData;
use embedded_hal_async::delay::DelayNs;
use embedded_io_async::{Read, Write};
use message::Measurement;
use message::Message;
use message::MessageType;
use message::Reporting;
use message::ReportingMode;
use message::Sleep;
use message::SleepMode;
use message::Version;
use message::WorkingPeriod;

mod message;

pub const RX_MSG_LEN: usize = 10;

pub enum SDS011Error<E> {
    ParseError(ParseError),
    ReadError(E),
    WriteError(E),
    ShortRead,
    ShortWrite,
    UnexpectedType,
    OperationFailed,
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

    pub struct Active;
    impl SensorState for Active {}

    pub struct Query;
    impl SensorState for Query {}

    pub struct Uninitialized;
    impl SensorState for Uninitialized {}
}

use sensor_trait::SensorState;
use sensor_trait::{Active, Query, Uninitialized};

pub struct SDS011<RW, S: SensorState> {
    serial: RW,
    sensor_id: Option<u16>,
    _state: PhantomData<S>,
}

impl<RW, S> SDS011<RW, S>
where
    RW: Read + Write,
    S: SensorState,
{
    async fn get_reply(&mut self) -> Result<Message, SDS011Error<RW::Error>> {
        let mut buf = [0u8; RX_MSG_LEN];

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

    async fn get_firmware(&mut self) -> Result<Version, SDS011Error<RW::Error>> {
        self.send_message(MessageType::FirmwareVersion(None))
            .await?;

        match self.get_reply().await?.m_type {
            MessageType::FirmwareVersion(data) => Ok(data.expect("replies always contain data")),
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
}

impl<RW> SDS011<RW, Uninitialized>
where
    RW: Read + Write,
{
    pub fn new(serial: RW) -> Self {
        SDS011::<RW, Uninitialized> {
            serial,
            sensor_id: None,
            _state: PhantomData,
        }
    }

    pub async fn init<D: DelayNs>(
        mut self,
        delay: &mut D,
    ) -> Result<SDS011<RW, Query>, SDS011Error<RW::Error>> {
        self.wake().await?;
        self.set_runmode_query().await?;
        self.sleep().await?;

        // sleep a short moment to make sure the sensor is ready (todo: make configurable)
        delay.delay_ms(1_000).await;

        Ok(SDS011::<RW, Query> {
            serial: self.serial,
            sensor_id: self.sensor_id,
            _state: PhantomData,
        })
    }
}

impl<RW> SDS011<RW, Active>
where
    RW: Read + Write,
{
    pub async fn measure(&mut self) -> Result<Measurement, SDS011Error<RW::Error>> {
        // waits for internal WorkingPeriod, then sends measurement
        self.read_sensor(false).await
    }

    pub async fn make_polling(self) -> Result<SDS011<RW, Query>, SDS011Error<RW::Error>> {
        unimplemented!("instead of make_polling, re-initialize the sensor.")
    }
}

impl<RW> SDS011<RW, Query>
where
    RW: Read + Write,
{
    pub async fn measure<D: DelayNs>(
        &mut self,
        delay: &mut D,
    ) -> Result<Measurement, SDS011Error<RW::Error>> {
        self.wake().await?;

        // need to spin up for a few secs before measurement
        delay.delay_ms(10_000).await;
        let res = self.read_sensor(true).await?;
        self.sleep().await?;

        // sleep a short moment to make sure the sensor is ready (todo: make configurable)
        delay.delay_ms(1_000).await;

        Ok(res)
    }

    pub async fn version<D: DelayNs>(
        &mut self,
        delay: &mut D,
    ) -> Result<Version, SDS011Error<RW::Error>> {
        self.wake().await?;
        let res = self.get_firmware().await?;
        self.sleep().await?;

        // sleep a short moment to make sure the sensor is ready (todo: make configurable)
        delay.delay_ms(1_000).await;

        Ok(res)
    }

    pub async fn make_periodic(
        mut self,
        period: u8,
    ) -> Result<SDS011<RW, Active>, SDS011Error<RW::Error>> {
        self.wake().await?;
        // todo: check period validity somewhere
        self.set_period(period).await?;
        self.set_runmode_active().await?;

        Ok(SDS011::<RW, Active> {
            serial: self.serial,
            sensor_id: self.sensor_id,
            _state: PhantomData,
        })
    }
}

#[cfg(test)]
mod tests {}
