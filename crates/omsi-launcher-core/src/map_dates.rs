//! Map calendar helpers used by the launcher.

fn is_leap_year(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// `date` (YYYY-MM-DD) fitted to a map's year span, as OMSI 2 starts a map on a day it
/// allows so that its AI lists, chrono and timetables fit: the same day in the nearest
/// allowed year (29 February becomes the 28th where that year has none). A date inside the
/// span, or one that is no date, is kept.
pub fn fit_date_to_years(date: &str, span: Option<(i32, i32)>) -> String {
    let mut fields = date.trim().splitn(3, '-').map(|f| f.trim().parse::<i32>());
    let (Some((from, to)), Some(Ok(year)), Some(Ok(month)), Some(Ok(day))) =
        (span, fields.next(), fields.next(), fields.next())
    else {
        return date.to_string();
    };
    if (from..=to).contains(&year) {
        return date.to_string();
    }
    let year = year.clamp(from, to);
    let day = if (month, day) == (2, 29) && !is_leap_year(year) {
        28
    } else {
        day
    };
    format!("{year:04}-{month:02}-{day:02}")
}
