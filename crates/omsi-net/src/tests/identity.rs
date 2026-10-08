use super::*;

#[test]
fn unicode_info_fixed_fields_fit_without_losing_paths_or_fleet_metadata() {
    for letter in ["界", "🚌"] {
        let mut p = pose(1.0);
        p.id = u32::MAX;
        p.name = letter.repeat(MAX_NAME);
        p.bus = format!("Vehicles/{}.bus", "a".repeat(247));
        p.figure = format!("Humans/{}.hum", "b".repeat(249));
        p.paint = letter.repeat(MAX_FIELD);
        p.line = letter.repeat(16);
        p.destination = letter.repeat(MAX_FIELD);
        p.tour = letter.repeat(MAX_FIELD);
        p.bus_identity = letter.repeat(16);
        p.paint_identity = letter.repeat(16);
        p.number = letter.repeat(MAX_FIELD);
        p.ident = letter.repeat(MAX_FIELD);
        p.texts = vec![letter.repeat(MAX_TEXT_LEN); MAX_TEXTS];
        p.freetex = vec![letter.repeat(MAX_FREETEX_LEN); MAX_FREETEX];
        let text = p.encode_info();
        assert!(text.len() <= MAX_DATAGRAM, "{} bytes", text.len());
        let q = Pose::decode_info(&text.split('|').collect::<Vec<_>>()).unwrap();
        assert_eq!((q.bus, q.figure, q.number, q.ident), (p.bus, p.figure, p.number, p.ident));
        assert_eq!((q.bus_identity, q.paint_identity), (p.bus_identity, p.paint_identity));
    }
}
