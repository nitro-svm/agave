#[derive(Clone, Copy, Default, Debug)]
pub struct Instant {}

impl Instant {
    pub fn now() -> Self {
        Self::default()
    }

    pub fn elapsed(&self) -> Duration {
        Duration::default()
    }

    pub fn duration_since(&self, _time: Instant) -> Duration {
        Duration::default()
    }
}

#[derive(
    Clone, Default, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct Duration {}

impl Duration {
    pub const fn new(_secs: u64, _nanos: u32) -> Duration {
        Duration {}
    }

    pub const fn from_nanos(_nanos: u64) -> Duration {
        Self::new(0, 0)
    }

    pub const fn from_micros(_micros: u64) -> Duration {
        Self::new(0, 0)
    }

    pub const fn from_millis(_millis: u64) -> Duration {
        Self::new(0, 0)
    }

    pub const fn from_secs(_secs: u64) -> Duration {
        Self::new(0, 0)
    }

    pub const fn as_secs(&self) -> u64 {
        0
    }

    pub const fn as_nanos(&self) -> u128 {
        0
    }

    pub const fn subsec_nanos(&self) -> u32 {
        0
    }

    pub const ZERO: Duration = Duration::from_nanos(0);
    pub const MAX: Duration = Duration::new(u64::MAX, 1);
}

#[derive(Default)]
pub struct SystemTime {}

impl SystemTime {
    pub fn now() -> SystemTime {
        Self::default()
    }

    pub fn duration_since(
        &self,
        _time: SystemTime,
    ) -> Result<Duration, Box<dyn std::error::Error>> {
        Ok(Duration::default())
    }
}

pub const UNIX_EPOCH: SystemTime = SystemTime {};
