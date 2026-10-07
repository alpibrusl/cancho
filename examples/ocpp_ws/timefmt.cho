edition 5;

module timefmt;

// `timefmt` -- an ISO 8601 UTC time (`2026-10-03T17:55:41.123Z`) from Unix milliseconds. OCPP's `currentTime` is one. `std` has no
// calendar: this is the civil-from-days algorithm (Howard Hinnant's), which is exact for every day from year 0 on.

fn put2[&o](out: &!o [byte], at: int, v: int) -> [] int {
    out[at] = byte_of('0' + v / 10 % 10);
    out[at + 1] = byte_of('0' + v % 10);
    return 0;
}

// Write the 24 bytes of `ms` into `out` at `at`. Answers 24.
pub fn iso[&o](out: &!o [byte], at: int, ms: int) -> [] int {
    let secs = ms / 1000;
    var days = secs / 86400;
    let rem = secs % 86400;
    // civil from days
    days = days + 719468;
    let era = days / 146097;
    let doe = days - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    var month = mp + 3;
    if mp >= 10 {
        month = mp - 9;
    }
    var year = yoe + era * 400;
    if month <= 2 {
        year = year + 1;
    }
    out[at] = byte_of('0' + year / 1000 % 10);
    out[at + 1] = byte_of('0' + year / 100 % 10);
    out[at + 2] = byte_of('0' + year / 10 % 10);
    out[at + 3] = byte_of('0' + year % 10);
    out[at + 4] = byte_of('-');
    put2(out, at + 5, month);
    out[at + 7] = byte_of('-');
    put2(out, at + 8, day);
    out[at + 10] = byte_of('T');
    put2(out, at + 11, rem / 3600);
    out[at + 13] = byte_of(':');
    put2(out, at + 14, rem / 60 % 60);
    out[at + 16] = byte_of(':');
    put2(out, at + 17, rem % 60);
    out[at + 19] = byte_of('.');
    let milli = ms % 1000;
    out[at + 20] = byte_of('0' + milli / 100);
    put2(out, at + 21, milli % 100);
    out[at + 23] = byte_of('Z');
    return 24;
}
