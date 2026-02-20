pub static REGISTERED_DIAGNOSTIC_CODES: &[&str] = &[
    "E0001", "E0002", "E0003", "E0004", "E0005", "E0006", "E1001", "E1002", "E1003", "E1004",
    "E1005", "E1006", "E1007", "E1008", "E1009", "E1010", "E1011", "E1012", "E1013", "E1014",
    "E1015", "E1016", "E1017", "E1018", "E1019", "E1020", "E1021", "E1022", "E1023", "E1024",
    "E1025", "E1026", "E1027", "E1028", "E1029", "E1030", "E1031", "E1032", "E1033", "E1034",
    "E1035", "E1036", "E1037", "E1038", "E1039", "E1040", "E1041", "E1042", "E1043", "E1044",
    "E1045", "E1046", "E1047", "E1048", "E1049", "E1050", "E1051", "E1052", "E1053", "E1054",
    "E1055", "E1056", "E1057", "E1058", "E1059", "E1060", "E1061", "E1062", "E1100", "E1101",
    "E1102", "E1103", "E1104", "E1105", "E1200", "E1201", "E1202", "E1203", "E1204", "E1205",
    "E1206", "E1207", "E1208", "E1209", "E1210", "E1211", "E1212", "E1213", "E1214", "E1215",
    "E1216", "E1217", "E1218", "E1219", "E1220", "E1221", "E1222", "E1223", "E1224", "E1225",
    "E1226", "E1227", "E1228", "E1229", "E1230", "E1231", "E1232", "E1233", "E1234", "E1235",
    "E1236", "E1237", "E1238", "E1239", "E1240", "E1241", "E1242", "E1243", "E1244", "E1245",
    "E1246", "E1247", "E1248", "E1249", "E1250", "E1251", "E1252", "E1253", "E1254", "E1255",
    "E1256", "E1257", "E1258", "E1259", "E1260", "E1261", "E1262", "E1263", "E1264", "E1265",
    "E1266", "E1267", "E1268", "E1269", "E1300", "E1301", "E2001", "E2002", "E2003", "E2004",
    "E2005", "E2100", "E2101", "E2102", "E2103", "E2104", "E2105", "E2106", "E2107", "E2108",
    "E2109", "E4001", "E4002", "E4003", "E4004", "E4005", "E5001", "E5002", "E5003", "E5004",
    "E5005", "E5006", "E5007", "E5008", "E5009", "E5010", "E5011", "E5012", "E5013", "E5014",
    "E5015", "E5016", "E5017", "E5018", "E5019", "E5020", "E5021", "E5022", "E6001", "E6002",
];

pub fn is_valid_format(code: &str) -> bool {
    let bytes = code.as_bytes();
    bytes.len() == 5
        && bytes[0] == b'E'
        && bytes[1].is_ascii_digit()
        && bytes[2].is_ascii_digit()
        && bytes[3].is_ascii_digit()
        && bytes[4].is_ascii_digit()
}

pub fn is_registered(code: &str) -> bool {
    REGISTERED_DIAGNOSTIC_CODES.binary_search(&code).is_ok()
}

pub fn assert_registered(code: &str) {
    assert!(
        is_valid_format(code),
        "invalid diagnostic code format: {code} (expected E####)"
    );
    assert!(is_registered(code), "unregistered diagnostic code: {code}");
}

#[cfg(test)]
mod tests {
    use super::{is_registered, is_valid_format, REGISTERED_DIAGNOSTIC_CODES};

    #[test]
    fn registry_is_sorted_and_unique() {
        for w in REGISTERED_DIAGNOSTIC_CODES.windows(2) {
            assert!(w[0] < w[1], "registry must be sorted and unique");
        }
    }

    #[test]
    fn known_code_is_registered() {
        assert!(is_registered("E1001"));
        assert!(is_valid_format("E5001"));
        assert!(!is_registered("E9999"));
    }
}
