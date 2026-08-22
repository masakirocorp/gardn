const MAX_RESPONSE_BYTES: usize = 4096;
const KITTY_PREFIX: &[u8] = b"\x1b_G";
const KITTY_SUFFIX: &[u8] = b"\x1b\\";
const RESPONSE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);
const LATE_RESPONSE_DRAIN: std::time::Duration = std::time::Duration::from_secs(1);

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Response {
    pub(super) transfer_id: u64,
    pub(super) image_id: u32,
    pub(super) success: bool,
}

#[derive(Debug, Default)]
pub(super) struct ResponseMatcher {
    expected: Option<(u64, u32, Option<std::time::Instant>)>,
    retired: Option<(u32, std::time::Instant)>,
}

impl ResponseMatcher {
    pub(super) fn arm(&mut self, transfer_id: u64, image_id: u32) -> bool {
        self.expire();
        if self.expected.is_some() {
            return false;
        }
        self.expected = Some((transfer_id, image_id, None));
        true
    }

    pub(super) fn start(&mut self, transfer_id: u64) {
        if let Some((id, _, deadline)) = &mut self.expected {
            if *id == transfer_id {
                *deadline = Some(std::time::Instant::now() + RESPONSE_TIMEOUT);
            }
        }
    }

    pub(super) fn retire(&mut self, transfer_id: u64) {
        if self.expected.is_some_and(|(id, _, _)| id == transfer_id) {
            if let Some((_, image_id, _)) = self.expected.take() {
                self.retired = Some((image_id, std::time::Instant::now() + LATE_RESPONSE_DRAIN));
            }
        }
    }

    pub(super) fn expire(&mut self) {
        let now = std::time::Instant::now();
        if self
            .expected
            .is_some_and(|(_, _, deadline)| deadline.is_some_and(|deadline| deadline <= now))
        {
            if let Some((_, image_id, _)) = self.expected.take() {
                self.retired = Some((image_id, now + LATE_RESPONSE_DRAIN));
            }
        }
        if self.retired.is_some_and(|(_, deadline)| deadline <= now) {
            self.retired = None;
        }
    }

    pub(super) fn consume(&mut self, bytes: &[u8]) -> Option<Option<Response>> {
        self.expire();
        if !bytes.starts_with(KITTY_PREFIX)
            || bytes.len() > MAX_RESPONSE_BYTES
            || !bytes.ends_with(KITTY_SUFFIX)
        {
            return None;
        }
        let payload = &bytes[3..bytes.len() - 2];
        let separator = payload.iter().position(|byte| *byte == b';')?;
        if let Some((retired_id, _)) = self.retired {
            if matching_response_controls(&payload[..separator], retired_id) {
                self.retired = None;
                return Some(None);
            }
        }
        let (transfer_id, image_id, _) = self.expected?;
        if !matching_response_controls(&payload[..separator], image_id) {
            return None;
        }
        self.expected = None;
        Some(Some(Response {
            transfer_id,
            image_id,
            success: &payload[separator + 1..] == b"OK",
        }))
    }
}

fn matching_response_controls(bytes: &[u8], expected: u32) -> bool {
    let mut matched = false;
    for field in bytes.split(|byte| *byte == b',') {
        let Some(separator) = field.iter().position(|byte| *byte == b'=') else {
            return false;
        };
        let (key, value) = field.split_at(separator);
        let value = &value[1..];
        if value.is_empty() || !value.iter().all(u8::is_ascii_digit) {
            return false;
        }
        match key {
            b"i" if !matched
                && std::str::from_utf8(value).ok().and_then(|v| v.parse().ok())
                    == Some(expected) =>
            {
                matched = true
            }
            b"I" | b"p" => {}
            _ => return false,
        }
    }
    matched
}

pub(super) fn valid_control(control: &str, image_id: u32) -> bool {
    if control.len() > 1024 || control.contains([';', '\x1b']) {
        return false;
    }
    let mut action = false;
    let mut format = false;
    let mut image = false;
    let mut quiet = false;
    let mut cursor = false;
    for field in control.split(',') {
        let Some((key, value)) = field.split_once('=') else {
            return false;
        };
        if key == "t"
            || !matches!(
                key,
                "a" | "f"
                    | "s"
                    | "v"
                    | "i"
                    | "p"
                    | "c"
                    | "r"
                    | "z"
                    | "C"
                    | "q"
                    | "x"
                    | "y"
                    | "w"
                    | "h"
                    | "X"
                    | "Y"
            )
        {
            return false;
        }
        let numeric = value
            .strip_prefix('-')
            .unwrap_or(value)
            .bytes()
            .all(|byte| byte.is_ascii_digit())
            && !value.is_empty();
        if key != "a" && !numeric {
            return false;
        }
        match key {
            "a" => action = value == "T",
            "f" => format = value == "32",
            "i" => image = value.parse() == Ok(image_id),
            "q" => quiet = value == "0",
            "C" => cursor = value == "1",
            _ => {}
        }
    }
    action && format && image && quiet && cursor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_only_the_armed_image_and_preserves_unrelated_input() {
        let mut matcher = ResponseMatcher::default();
        assert!(matcher.arm(7, 42));
        assert!(!matcher.arm(8, 43));
        assert_eq!(matcher.consume(b"typed"), None);
        assert_eq!(matcher.consume(b"\x1b_Gi=41;OK\x1b\\"), None);
        assert_eq!(
            matcher.consume(b"\x1b_Gi=42;OK\x1b\\"),
            Some(Some(Response {
                transfer_id: 7,
                image_id: 42,
                success: true,
            }))
        );
    }

    #[test]
    fn validated_control_is_one_owned_rgba_transmit_and_display() {
        assert!(valid_control(
            "a=T,f=32,s=10,v=20,i=42,p=7,c=5,r=6,z=-1,C=1,q=0,x=2",
            42
        ));
        for invalid in [
            "a=T,f=24,i=42,C=1,q=0",
            "a=T,f=32,i=41,C=1,q=0",
            "a=T,t=f,f=32,i=42,C=1,q=0",
            "a=p,f=32,i=42,C=1,q=0",
        ] {
            assert!(!valid_control(invalid, 42), "{invalid}");
        }
    }
}
