use chrono::{DateTime, NaiveDate, Utc};

pub fn zero_time() -> DateTime<Utc> {
    // Create a NaiveDateTime for the Unix epoch (1970-01-01T00:00:00)
    let naive = NaiveDate::from_ymd(1970, 1, 1).and_hms(0, 0, 0);
    // Convert NaiveDateTime to DateTime<Utc>
    DateTime::<Utc>::from_utc(naive, Utc)
}

pub fn max_time() -> DateTime<Utc> {
    // Create a NaiveDateTime for the Unix epoch (1970-01-01T00:00:00)
    let naive = NaiveDate::from_ymd(9999, 12, 31).and_hms(23, 59, 59);
    // Convert NaiveDateTime to DateTime<Utc>
    DateTime::<Utc>::from_utc(naive, Utc)
}
