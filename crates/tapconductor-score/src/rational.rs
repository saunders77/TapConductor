use std::{cmp::Ordering, fmt, str::FromStr};

use serde::{de, Deserialize, Deserializer, Serialize};

use crate::RationalError;

/// A reduced exact rational number with a strictly positive denominator.
///
/// Musical positions must never be compared as floating-point values. Construction and arithmetic
/// are checked so malformed scores cannot silently wrap their timeline.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
pub struct Rational {
    numerator: i64,
    denominator: i64,
}

impl Rational {
    pub const ZERO: Self = Self {
        numerator: 0,
        denominator: 1,
    };
    pub const ONE: Self = Self {
        numerator: 1,
        denominator: 1,
    };

    pub fn new(numerator: i64, denominator: i64) -> Result<Self, RationalError> {
        Self::from_i128(i128::from(numerator), i128::from(denominator))
    }

    pub const fn from_integer(value: i64) -> Self {
        Self {
            numerator: value,
            denominator: 1,
        }
    }

    pub const fn numerator(self) -> i64 {
        self.numerator
    }

    pub const fn denominator(self) -> i64 {
        self.denominator
    }

    pub const fn is_zero(self) -> bool {
        self.numerator == 0
    }

    pub const fn is_negative(self) -> bool {
        self.numerator < 0
    }

    pub fn checked_add(self, rhs: Self) -> Result<Self, RationalError> {
        let numerator = i128::from(self.numerator) * i128::from(rhs.denominator)
            + i128::from(rhs.numerator) * i128::from(self.denominator);
        let denominator = i128::from(self.denominator) * i128::from(rhs.denominator);
        Self::from_i128(numerator, denominator)
    }

    pub fn checked_sub(self, rhs: Self) -> Result<Self, RationalError> {
        let numerator = i128::from(self.numerator) * i128::from(rhs.denominator)
            - i128::from(rhs.numerator) * i128::from(self.denominator);
        let denominator = i128::from(self.denominator) * i128::from(rhs.denominator);
        Self::from_i128(numerator, denominator)
    }

    pub fn checked_mul(self, rhs: Self) -> Result<Self, RationalError> {
        Self::from_i128(
            i128::from(self.numerator) * i128::from(rhs.numerator),
            i128::from(self.denominator) * i128::from(rhs.denominator),
        )
    }

    pub fn checked_mul_i64(self, rhs: i64) -> Result<Self, RationalError> {
        Self::from_i128(
            i128::from(self.numerator) * i128::from(rhs),
            i128::from(self.denominator),
        )
    }

    pub fn checked_div(self, rhs: Self) -> Result<Self, RationalError> {
        if rhs.numerator == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        Self::from_i128(
            i128::from(self.numerator) * i128::from(rhs.denominator),
            i128::from(self.denominator) * i128::from(rhs.numerator),
        )
    }

    /// Parse an integer or non-scientific decimal exactly (for example MusicXML `alter` values).
    pub fn parse_decimal(value: &str) -> Result<Self, RationalError> {
        let value = value.trim();
        if value.is_empty() {
            return Err(RationalError::InvalidNumber("empty value"));
        }

        let (negative, unsigned) = match value.as_bytes()[0] {
            b'-' => (true, &value[1..]),
            b'+' => (false, &value[1..]),
            _ => (false, value),
        };
        if unsigned.is_empty() {
            return Err(RationalError::InvalidNumber("missing digits"));
        }

        let mut split = unsigned.split('.');
        let whole = split.next().unwrap_or_default();
        let fraction = split.next();
        if split.next().is_some()
            || whole.bytes().any(|byte| !byte.is_ascii_digit())
            || fraction.is_some_and(|digits| digits.bytes().any(|byte| !byte.is_ascii_digit()))
            || (whole.is_empty() && fraction.is_none_or(str::is_empty))
        {
            return Err(RationalError::InvalidNumber(
                "expected an integer or decimal",
            ));
        }

        let fraction = fraction.unwrap_or_default();
        let denominator = 10_i128
            .checked_pow(
                fraction
                    .len()
                    .try_into()
                    .map_err(|_| RationalError::Overflow)?,
            )
            .ok_or(RationalError::Overflow)?;
        let whole_value = if whole.is_empty() {
            0
        } else {
            whole.parse::<i128>().map_err(|_| RationalError::Overflow)?
        };
        let fraction_value = if fraction.is_empty() {
            0
        } else {
            fraction
                .parse::<i128>()
                .map_err(|_| RationalError::Overflow)?
        };
        let mut numerator = whole_value
            .checked_mul(denominator)
            .and_then(|number| number.checked_add(fraction_value))
            .ok_or(RationalError::Overflow)?;
        if negative {
            numerator = -numerator;
        }
        Self::from_i128(numerator, denominator)
    }

    fn from_i128(mut numerator: i128, mut denominator: i128) -> Result<Self, RationalError> {
        if denominator == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        if denominator < 0 {
            numerator = numerator.checked_neg().ok_or(RationalError::Overflow)?;
            denominator = denominator.checked_neg().ok_or(RationalError::Overflow)?;
        }
        if numerator == 0 {
            return Ok(Self::ZERO);
        }

        let divisor = gcd(numerator.unsigned_abs(), denominator as u128) as i128;
        numerator /= divisor;
        denominator /= divisor;
        if numerator < i128::from(i64::MIN)
            || numerator > i128::from(i64::MAX)
            || denominator > i128::from(i64::MAX)
        {
            return Err(RationalError::Overflow);
        }
        Ok(Self {
            numerator: numerator as i64,
            denominator: denominator as i64,
        })
    }
}

fn gcd(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

impl Default for Rational {
    fn default() -> Self {
        Self::ZERO
    }
}

impl Ord for Rational {
    fn cmp(&self, other: &Self) -> Ordering {
        (i128::from(self.numerator) * i128::from(other.denominator))
            .cmp(&(i128::from(other.numerator) * i128::from(self.denominator)))
    }
}

impl PartialOrd for Rational {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Rational {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.denominator == 1 {
            write!(formatter, "{}", self.numerator)
        } else {
            write!(formatter, "{}/{}", self.numerator, self.denominator)
        }
    }
}

impl FromStr for Rational {
    type Err = RationalError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if let Some((numerator, denominator)) = value.split_once('/') {
            let numerator = numerator
                .trim()
                .parse::<i64>()
                .map_err(|_| RationalError::InvalidNumber("invalid numerator"))?;
            let denominator = denominator
                .trim()
                .parse::<i64>()
                .map_err(|_| RationalError::InvalidNumber("invalid denominator"))?;
            Self::new(numerator, denominator)
        } else {
            Self::parse_decimal(value)
        }
    }
}

impl<'de> Deserialize<'de> for Rational {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Representation {
            numerator: i64,
            denominator: i64,
        }

        let value = Representation::deserialize(deserializer)?;
        Self::new(value.numerator, value.denominator).map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduces_and_normalizes_sign() {
        assert_eq!(Rational::new(6, -8).unwrap(), Rational::new(-3, 4).unwrap());
        assert_eq!(Rational::new(0, -99).unwrap(), Rational::ZERO);
    }

    #[test]
    fn compares_without_floating_point() {
        assert!(Rational::new(1, 3).unwrap() < Rational::new(2, 5).unwrap());
        assert_eq!(
            Rational::new(100, 300).unwrap(),
            Rational::new(1, 3).unwrap()
        );
    }

    #[test]
    fn arithmetic_is_checked_and_reduced() {
        let one_third = Rational::new(1, 3).unwrap();
        let one_sixth = Rational::new(1, 6).unwrap();
        assert_eq!(
            one_third.checked_add(one_sixth).unwrap(),
            Rational::new(1, 2).unwrap()
        );
        assert_eq!(one_third.checked_sub(one_sixth).unwrap(), one_sixth);
        assert_eq!(
            one_third.checked_mul_i64(6).unwrap(),
            Rational::from_integer(2)
        );
    }

    #[test]
    fn parses_decimal_exactly() {
        assert_eq!(
            Rational::parse_decimal("-0.50").unwrap(),
            Rational::new(-1, 2).unwrap()
        );
        assert_eq!(
            Rational::parse_decimal("4").unwrap(),
            Rational::from_integer(4)
        );
        assert!(Rational::parse_decimal("1e-3").is_err());
    }

    #[test]
    fn deserialize_rejects_invalid_invariant() {
        let result = serde_json::from_str::<Rational>(r#"{"numerator":1,"denominator":0}"#);
        assert!(result.is_err());
    }
}
