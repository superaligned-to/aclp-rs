use proptest::collection::vec;
use proptest::prelude::*;
use serde_json::json;

use super::*;

#[test]
fn limits_keep_their_documented_values() {
    assert_eq!(MAX_BODY_BYTES, 1_048_576);
    assert_eq!(MAX_JSON_DEPTH, 64);
    assert_eq!(MAX_JSON_NODES, 100_000);
}

#[test]
fn decoding_preserves_values_and_member_order() {
    let src = br#"{"z":1,"a":[true,null,"x",-2.5,18446744073709551615]}"#;

    let val = decode(src).expect("valid JSON");

    assert_eq!(serde_json::to_vec(&val).expect("encodes"), src.to_vec());
}

#[test]
fn decoding_keeps_every_bit_of_a_float() {
    for src in [
        "1.0715660391465826e-75",
        "-1.81996730402717e-179",
        "-1.603964615428183e143",
    ] {
        let want: f64 = src.parse().expect("float literal");

        let got = decode(src.as_bytes()).ok().and_then(|v| v.as_f64());

        assert_eq!(got.map(f64::to_bits), Some(want.to_bits()), "{src}");
    }
}

#[test]
fn decoding_rejects_duplicate_keys_invalid_utf8_and_non_finite_numbers() {
    for src in [
        r#"{"x":1,"x":2}"#,
        r#"{"x":1,"x":2}"#,
        "1e400",
        r#"{"n":NaN}"#,
        "{} trailing",
        r#"{"x":1,}"#,
        "",
    ] {
        assert!(decode(src.as_bytes()).is_err(), "{src}");
    }

    assert!(decode(&[255]).is_err());
    assert!(decode(&[239, 187, 191, b'{', b'}']).is_err());
    assert!(decode(b"\"\xff\"").is_err());
}

#[test]
fn body_and_complexity_limits_reject_one_over_boundary_input() {
    let nested = |depth: usize| format!("{}0{}", "[".repeat(depth), "]".repeat(depth));

    assert!(decode(nested(MAX_JSON_DEPTH).as_bytes()).is_ok());
    assert_eq!(
        decode(nested(MAX_JSON_DEPTH + 1).as_bytes()),
        Err(JsonError::TooComplex)
    );
    assert_eq!(
        decode(&vec![b' '; MAX_BODY_BYTES + 1]),
        Err(JsonError::TooLarge)
    );

    let wide = format!("[{}0]", "0,".repeat(MAX_JSON_NODES - 2));

    assert!(decode(wide.as_bytes()).is_ok());

    let wider = format!("[{}0]", "0,".repeat(MAX_JSON_NODES - 1));

    assert_eq!(decode(wider.as_bytes()), Err(JsonError::TooComplex));
}

#[test]
fn encoding_enforces_the_same_bounds() {
    let exact = Value::from("x".repeat(MAX_BODY_BYTES - 2));

    assert_eq!(encode(&exact).map(|b| b.len()), Ok(MAX_BODY_BYTES));
    assert_eq!(
        encode(&Value::from("x".repeat(MAX_BODY_BYTES - 1))),
        Err(JsonError::TooLarge)
    );
    assert_eq!(
        encode(&json!(vec![0; MAX_JSON_NODES])),
        Err(JsonError::TooComplex)
    );
}

fn bounded() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::from),
        any::<i64>().prop_map(Value::from),
        any::<u64>().prop_map(Value::from),
        any::<u64>()
            .prop_map(f64::from_bits)
            .prop_filter("finite", |f| f.is_finite())
            .prop_map(Value::from),
        any::<String>().prop_map(Value::from),
    ];

    leaf.prop_recursive(8, 256, 8, |inner| {
        prop_oneof![
            vec(inner.clone(), 0..8).prop_map(Value::Array),
            vec((any::<String>(), inner), 0..8)
                .prop_map(|members| Value::Object(members.into_iter().collect())),
        ]
    })
}

proptest! {
    #[test]
    fn arbitrary_bytes_never_panic(bytes in vec(any::<u8>(), 0..512)) {
        let _ = decode(&bytes);
    }

    #[test]
    fn corrupted_json_never_panics(val in bounded(), idx in any::<usize>(), byte in any::<u8>()) {
        let mut bytes = encode(&val).expect("bounded value encodes");

        if !bytes.is_empty() {
            let at = idx % bytes.len();

            bytes[at] = byte;
        }

        let _ = decode(&bytes);
    }

    #[test]
    fn bounded_values_round_trip_exactly(val in bounded()) {
        let bytes = encode(&val).expect("bounded value encodes");

        prop_assert_eq!(decode(&bytes), Ok(val));
    }
}
