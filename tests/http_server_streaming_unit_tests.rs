use std::fs;

use aicore::parser::parse;

fn assert_function_signature(source: &str, signature: &str) {
    assert!(
        source.contains(signature),
        "missing signature fragment: {signature}"
    );
}

#[test]
fn unit_http_server_streaming_public_surface_exists() {
    let source = fs::read_to_string("std/http_server.aic").expect("read std/http_server.aic");
    let (program, diags) = parse(&source, "std/http_server.aic");
    assert!(diags.is_empty(), "parse diagnostics: {diags:#?}");
    assert!(program.is_some(), "expected parsed program");

    assert_function_signature(&source, "struct RequestBodyReader {");
    assert_function_signature(&source, "struct RequestBodyStep {");
    assert_function_signature(&source, "struct ResponseBodyWriter {");
    assert_function_signature(&source, "fn read_request_stream(");
    assert_function_signature(
        &source,
        "fn request_stream_head(reader: RequestBodyReader) -> RequestHead",
    );
    assert_function_signature(
        &source,
        "fn request_stream_read_next(reader: RequestBodyReader) -> Result[RequestBodyStep, ServerError]",
    );
    assert_function_signature(
        &source,
        "fn request_stream_done(reader: RequestBodyReader) -> Bool",
    );
    assert_function_signature(&source, "fn begin_streaming_response(");
    assert_function_signature(&source, "fn write_streaming_response_chunk(");
    assert_function_signature(
        &source,
        "fn finish_streaming_response(writer: ResponseBodyWriter) -> Result[Int, ServerError]",
    );
}

#[test]
fn unit_http_server_streaming_helpers_encode_expected_rules() {
    let source = fs::read_to_string("std/http_server.aic").expect("read std/http_server.aic");

    assert_function_signature(
        &source,
        "fn http_server_positive_chunk_bytes(chunk_bytes: Int) -> Int",
    );
    assert!(
        source.contains("if chunk_bytes <= 0"),
        "chunk size guard missing"
    );
    assert!(source.contains("chunk_bytes: http_server_positive_chunk_bytes(chunk_bytes)"));
    assert!(
        source.contains("chunk_len > remaining"),
        "overflow guard missing"
    );
    assert!(
        source.contains("Err(BodyTooLarge())"),
        "overflow error mapping missing"
    );
    assert!(
        source.contains("Err(InvalidHeader())"),
        "negative content-length guard missing"
    );
    assert!(source.contains("if writer.sent_bytes != writer.expected_bytes"));
    assert!(
        source.contains("Err(InvalidRequest())"),
        "finish underflow error mapping missing"
    );
    assert!(
        source.contains("fn http_server_net_error_to_server_error(err: NetError) -> ServerError")
    );
    assert!(source.contains("Timeout => Timeout()"));
    assert!(source.contains("ConnectionClosed => ConnectionClosed()"));
    assert!(source.contains("InvalidInput => InvalidRequest()"));
    assert!(source.contains("fn http_server_send_bytes("));
    assert!(source.contains("match net.tcp_send_timeout(conn, payload, timeout_ms)"));
}
