use xxhash_rust::xxh64::Xxh64;
use fff_core::errors::{Error, Result};

#[repr(u8)]
#[derive(PartialEq, Debug, Clone, Copy)]
pub enum ChecksumType {
    XxHash,
}

impl TryFrom<u8> for ChecksumType {
    type Error = Error;

    fn try_from(v: u8) -> Result<ChecksumType> {
        match v {
            0 => Ok(ChecksumType::XxHash),
            _ => Err(Error::General(format!("Invalid checksum type: {}", v))),
        }
    }
}

pub trait Checksum {
    fn update(&mut self, data: &[u8]);
    fn finalize(&self) -> u64;
    fn reset(&mut self);
}

#[derive(Default)]
pub struct XxHash {
    state: Xxh64,
}

impl Checksum for XxHash {
    fn update(&mut self, data: &[u8]) {
        self.state.update(data);
    }

    fn finalize(&self) -> u64 {
        self.state.digest()
    }

    fn reset(&mut self) {
        self.state = Xxh64::default()
    }
}

pub fn create_checksum(checksum_type: &ChecksumType) -> Box<dyn Checksum> {
    match checksum_type {
        ChecksumType::XxHash => Box::new(XxHash::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xxhash() {
        let mut checksum = create_checksum(&ChecksumType::XxHash);
        checksum.update(b"helloworld");
        let c1 = checksum.finalize();

        let mut checksum = create_checksum(&ChecksumType::XxHash);
        checksum.update(b"hello");
        checksum.update(b"world");
        let c2 = checksum.finalize();
        assert_eq!(c1, c2);

        let mut checksum = create_checksum(&ChecksumType::XxHash);
        checksum.update(b"hell");
        checksum.update(b"oworld");
        let c3 = checksum.finalize();

        assert_eq!(c1, c3);

        let mut checksum = create_checksum(&ChecksumType::XxHash);
        checksum.update(b"oworld");
        checksum.update(b"hell");
        let c4 = checksum.finalize();
        assert_ne!(c3, c4);
    }

    #[test]
    fn test_checksum_type_from_u8_valid() {
        // Test valid checksum type
        let checksum_type = ChecksumType::try_from(0u8);
        assert!(checksum_type.is_ok());
        assert_eq!(checksum_type.unwrap(), ChecksumType::XxHash);
    }

    #[test]
    fn test_checksum_type_from_u8_invalid() {
        // Test invalid checksum types should return error, not panic
        let invalid_values = [1u8, 2, 10, 100, 255];
        for value in invalid_values {
            let result = ChecksumType::try_from(value);
            assert!(result.is_err(), "Expected error for value {}", value);
        }
    }

    #[test]
    fn test_checksum_type_roundtrip() {
        // Test that we can convert to u8 and back
        let original = ChecksumType::XxHash;
        let as_u8 = original as u8;
        let back = ChecksumType::try_from(as_u8).unwrap();
        assert_eq!(original, back);
    }
}
