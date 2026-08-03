use anyhow::{bail, Context, Result};
use std::fmt;
use std::str::FromStr;

pub const MIN_BPM_HUNDREDTHS: u16 = 2_000;
pub const MAX_BPM_HUNDREDTHS: u16 = 30_000;

/// A deterministic Project/transport tempo stored as hundredths of a BPM.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Bpm(u16);

impl Bpm {
    pub const DEFAULT: Self = Self(12_000);

    pub const fn from_hundredths(hundredths: u16) -> Option<Self> {
        if hundredths >= MIN_BPM_HUNDREDTHS && hundredths <= MAX_BPM_HUNDREDTHS {
            Some(Self(hundredths))
        } else {
            None
        }
    }

    pub fn from_hundredths_clamped(hundredths: u16) -> Self {
        Self(hundredths.clamp(MIN_BPM_HUNDREDTHS, MAX_BPM_HUNDREDTHS))
    }

    pub const fn from_whole(bpm: u16) -> Option<Self> {
        match bpm.checked_mul(100) {
            Some(hundredths) => Self::from_hundredths(hundredths),
            None => None,
        }
    }

    pub const fn hundredths(self) -> u16 {
        self.0
    }

    pub fn as_f64(self) -> f64 {
        f64::from(self.0) / 100.0
    }

    /// Adjust by ordinary whole-BPM UI steps without discarding the fraction.
    pub fn adjust_whole(self, steps: i16) -> Self {
        let delta = (steps as i32) * 100;
        let next =
            (self.0 as i32 + delta).clamp(MIN_BPM_HUNDREDTHS as i32, MAX_BPM_HUNDREDTHS as i32);
        Self(next as u16)
    }

    pub fn from_micros_per_quarter(micros: u32) -> Result<Self> {
        if micros == 0 {
            bail!("MIDI tempo must be greater than zero");
        }
        let rounded = (6_000_000_000u64 + u64::from(micros) / 2) / u64::from(micros);
        let hundredths = u16::try_from(rounded).context("MIDI tempo is outside the BPM range")?;
        Self::from_hundredths(hundredths).context("MIDI tempo must be 20.00..=300.00 BPM")
    }
}

impl Default for Bpm {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl fmt::Display for Bpm {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_multiple_of(100) {
            write!(formatter, "{}", self.0 / 100)
        } else {
            write!(formatter, "{}.{:02}", self.0 / 100, self.0 % 100)
        }
    }
}

impl FromStr for Bpm {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        let value = value.trim();
        if value.is_empty() || value.starts_with('-') || value.starts_with('+') {
            bail!("tempo must be a decimal number");
        }
        let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
        let whole = whole
            .parse::<u16>()
            .context("tempo must be a decimal number")?;
        if !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
            bail!("tempo must be a decimal number");
        }
        let digits = fraction.as_bytes();
        let fraction = match digits.len() {
            0 => 0,
            1 => u16::from(digits[0] - b'0') * 10,
            2 => u16::from(digits[0] - b'0') * 10 + u16::from(digits[1] - b'0'),
            _ if digits[2..].iter().all(|byte| *byte == b'0') => {
                u16::from(digits[0] - b'0') * 10 + u16::from(digits[1] - b'0')
            }
            _ => bail!("tempo supports at most two decimal places"),
        };
        let hundredths = whole
            .checked_mul(100)
            .and_then(|value| value.checked_add(fraction))
            .context("tempo is outside the BPM range")?;
        Self::from_hundredths(hundredths).context("tempo must be 20.00..=300.00 BPM")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_tempo_is_exact_and_whole_steps_keep_its_fraction() {
        let tempo = "100.50".parse::<Bpm>().unwrap();
        assert_eq!(tempo.hundredths(), 10_050);
        assert_eq!(tempo.to_string(), "100.50");
        assert_eq!(tempo.adjust_whole(1).to_string(), "101.50");
        assert_eq!(tempo.adjust_whole(-2).to_string(), "98.50");
        assert_eq!("120".parse::<Bpm>().unwrap().to_string(), "120");
    }

    #[test]
    fn midi_microseconds_round_to_deterministic_hundredths() {
        assert_eq!(
            Bpm::from_micros_per_quarter(714_286).unwrap(),
            "84".parse().unwrap()
        );
        assert_eq!(
            Bpm::from_micros_per_quarter(597_015).unwrap(),
            "100.50".parse().unwrap()
        );
    }
}
