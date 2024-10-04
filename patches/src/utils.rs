#[derive(Default)]
pub struct Instant {}

impl Instant {
    pub fn now() -> Self {
        Self::Default()
    }

    pub fn elapsed(&self) -> Duration {
        Duration::Default()
    }
}

#[derive(Default, Debug, PartialEq, Eq)]
pub struct Duration {}

impl Duration {
    pub const fn new(_secs: u64, _nanos: u32) -> Duration {
        Self::Default()
    }

    pub const fn from_nanos(_nanos: u64) -> Duration {
        Self::Default()
    }
    pub const fn from_micros(_micros: u64) -> Duration {
        Self::Default()
    }
    pub const fn from_millis(_millis: u64) -> Duration {
        Self::Default()
    }
    pub const fn from_secs(_secs: u64) -> Duration {
        Self::Default()
    }
    
    pub const ZERO: Duration = Duration::from_nanos(0);
    pub const MAX: Duration = Duration::new(u64::MAX, 1);
}
