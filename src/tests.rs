use super::*;

#[cfg(feature = "rtt")]
#[test]
fn explicit_jlink_ip_ignores_saved_and_env_serial_numbers() {
    assert_eq!(
        select_jlink_sn(false, true, None, Some(111), Some(222)),
        None
    );
    assert_eq!(
        select_jlink_sn(false, true, Some(333), Some(111), Some(222)),
        None
    );
}

#[cfg(feature = "rtt")]
#[test]
fn direct_usb_jlink_uses_cli_env_then_config_serial_number() {
    assert_eq!(
        select_jlink_sn(false, false, Some(333), Some(111), Some(222)),
        Some(333)
    );
    assert_eq!(
        select_jlink_sn(false, false, None, Some(111), Some(222)),
        Some(111)
    );
    assert_eq!(
        select_jlink_sn(false, false, None, None, Some(222)),
        Some(222)
    );
    assert_eq!(
        select_jlink_sn(true, false, Some(333), Some(111), Some(222)),
        None
    );
}

#[test]
fn parse_control_write_args_defaults_to_current() {
    let args = parse_control_write_args("hello").unwrap();
    assert_eq!(args.target, ControlTarget::Current);
    assert!(!args.json);
    assert_eq!(
        args.timeout,
        Duration::from_millis(CONTROL_WRITE_ACK_TIMEOUT_MS)
    );
    assert_eq!(args.payload, "hello");
}

#[test]
fn parse_control_write_args_accepts_explicit_target_and_json() {
    let args = parse_control_write_args("--target rtt --timeout 123 --json AT+CFUN?").unwrap();
    assert_eq!(args.target, ControlTarget::Rtt);
    assert_eq!(args.timeout, Duration::from_millis(123));
    assert!(args.json);
    assert_eq!(args.payload, "AT+CFUN?");
}

#[test]
fn parse_control_write_args_accepts_dash_dash_payload() {
    let args = parse_control_write_args("--target serial --json -- --literal").unwrap();
    assert_eq!(args.target, ControlTarget::Serial);
    assert!(args.json);
    assert_eq!(args.payload, "--literal");
}

#[test]
fn parse_control_write_args_rejects_flag_like_payload_without_separator() {
    let err = parse_control_write_args("--json --literal")
        .unwrap_err()
        .to_string();
    assert!(err.contains("must follow the -- separator"));
}

#[test]
fn parse_control_request_args_accepts_target_and_timeout() {
    let args = parse_control_request_args("--target serial --timeout 750 AT").unwrap();
    assert_eq!(args.target, ControlTarget::Serial);
    assert_eq!(args.timeout, Duration::from_millis(750));
    assert_eq!(args.since, None);
    assert_eq!(args.until_hex, None);
    assert_eq!(args.payload, "AT");
    assert!(!args.hex);
    assert!(!args.json);
}

#[test]
fn parse_control_request_args_accepts_hex_payload() {
    let args =
        parse_control_request_args("--target rtt --timeout 100 --since 42 --hex 4154").unwrap();
    assert_eq!(args.target, ControlTarget::Rtt);
    assert_eq!(args.timeout, Duration::from_millis(100));
    assert_eq!(args.since, Some(ControlSince::Seq(42)));
    assert_eq!(args.payload, "4154");
    assert!(args.hex);
    assert!(!args.json);
}

#[test]
fn parse_control_request_args_accepts_json_output() {
    let args = parse_control_request_args(
        "--timeout 100 --since 42 --until-hex 4f4b --max-bytes 32 --fail-on-timeout --json AT",
    )
    .unwrap();
    assert_eq!(args.timeout, Duration::from_millis(100));
    assert_eq!(args.since, Some(ControlSince::Seq(42)));
    assert_eq!(args.until_hex, Some(b"OK".to_vec()));
    assert_eq!(args.max_bytes, Some(32));
    assert!(args.fail_on_timeout);
    assert_eq!(args.payload, "AT");
    assert!(!args.hex);
    assert!(args.json);
}

#[test]
fn parse_control_request_args_accepts_since_now() {
    let args = parse_control_request_args("--timeout 100 --since now --json AT").unwrap();
    assert_eq!(args.since, Some(ControlSince::Now));
    assert!(args.json);
}

#[test]
fn parse_control_request_args_accepts_dash_dash_payload() {
    let args =
        parse_control_request_args("--target serial --timeout 100 --json -- --literal").unwrap();
    assert_eq!(args.target, ControlTarget::Serial);
    assert_eq!(args.payload, "--literal");
    assert!(args.json);
}

#[test]
fn parse_control_request_args_rejects_flag_like_payload_without_separator() {
    let err = parse_control_request_args("--json --literal")
        .unwrap_err()
        .to_string();
    assert!(err.contains("must follow the -- separator"));
}

#[test]
fn parse_control_read_args_accepts_timeout() {
    let opts = parse_control_read_args("--timeout 250", Duration::from_millis(200)).unwrap();
    assert_eq!(opts.timeout, Duration::from_millis(250));
    assert_eq!(opts.since, None);
    assert!(!opts.json);
}

#[test]
fn parse_control_read_args_accepts_json_output() {
    let opts = parse_control_read_args(
        "--timeout 250 --since 10 --until-hex 0d0a --max-bytes 16 --fail-on-timeout --json",
        Duration::from_millis(200),
    )
    .unwrap();
    assert_eq!(opts.timeout, Duration::from_millis(250));
    assert_eq!(opts.since, Some(ControlSince::Seq(10)));
    assert_eq!(opts.until_hex, Some(b"\r\n".to_vec()));
    assert_eq!(opts.max_bytes, Some(16));
    assert!(opts.fail_on_timeout);
    assert!(opts.json);
}

#[test]
fn parse_control_read_args_accepts_since_now() {
    let opts = parse_control_read_args(
        "--timeout 250 --since now --json",
        Duration::from_millis(200),
    )
    .unwrap();
    assert_eq!(opts.since, Some(ControlSince::Now));
    assert!(opts.json);
}

#[test]
fn parse_control_read_args_rejects_glued_flags() {
    let err = parse_control_read_args("--jsonfoo", Duration::from_millis(200))
        .unwrap_err()
        .to_string();
    assert!(err.contains("unknown read option"));
}

#[test]
fn parse_control_read_args_rejects_positional_timeout() {
    let err = parse_control_read_args("250", Duration::from_millis(200))
        .unwrap_err()
        .to_string();
    assert!(err.contains("unknown read option"));
}

#[test]
fn parse_control_read_args_reports_missing_option_values() {
    for (input, expected) in [
        ("--timeout", "missing --timeout value"),
        ("--since", "missing --since value"),
        ("--until-hex", "missing --until-hex value"),
        ("--max-bytes", "missing --max-bytes value"),
    ] {
        let err = parse_control_read_args(input, Duration::from_millis(200))
            .unwrap_err()
            .to_string();
        assert!(err.contains(expected), "{input}: {err}");
    }
}

#[test]
fn parse_control_request_args_reports_missing_option_values() {
    for (input, expected) in [
        ("--target", "missing --target value"),
        ("--timeout", "missing --timeout value"),
        ("--since", "missing --since value"),
        ("--until-hex", "missing --until-hex value"),
        ("--max-bytes", "missing --max-bytes value"),
    ] {
        let err = parse_control_request_args(input).unwrap_err().to_string();
        assert!(err.contains(expected), "{input}: {err}");
    }
}

#[test]
fn parse_control_timeout_rejects_excessive_value() {
    let err = parse_control_read_args(
        &format!("--timeout {}", CONTROL_MAX_TIMEOUT_MS + 1),
        Duration::from_millis(200),
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("exceeds maximum"));
}

#[test]
fn parse_hex_bytes_accepts_common_separators() {
    assert_eq!(parse_hex_bytes("41 54-0d:0a").unwrap(), b"AT\r\n");
}

#[test]
fn server_cli_uses_socket_without_control_socket_alias() {
    let opts = Opts::try_parse_from(["rttio", "--socket", "custom.sock"]).unwrap();
    assert_eq!(opts.socket, PathBuf::from("custom.sock"));

    let err = Opts::try_parse_from(["rttio", "--control-socket", "custom.sock"])
        .expect_err("old --control-socket alias must not be accepted");
    assert_eq!(err.kind(), clap::error::ErrorKind::UnknownArgument);
}

#[test]
fn ctl_flash_cli_uses_positional_address() {
    let opts = Opts::try_parse_from([
        "rttio",
        "ctl",
        "flash",
        "--json",
        "--timeout",
        "123",
        "app.hex",
        "0x1000",
    ])
    .unwrap();

    let Some(Command::Ctl(ctl)) = opts.command else {
        panic!("expected ctl command");
    };
    let CtlCommand::Flash {
        json,
        file,
        addr,
        timeout,
    } = ctl.command
    else {
        panic!("expected flash command");
    };

    assert!(json);
    assert_eq!(file, PathBuf::from("app.hex"));
    assert_eq!(addr, 0x1000);
    assert_eq!(timeout, 123);
}

#[test]
fn flash_address_prompt_is_only_for_raw_bin() {
    assert!(flash_file_uses_embedded_address(Path::new(
        "build/merged.hex"
    )));
    assert!(flash_file_uses_embedded_address(Path::new("build/app.elf")));
    assert!(flash_file_uses_embedded_address(Path::new("build/app.uf2")));
    assert!(!flash_file_uses_embedded_address(Path::new(
        "build/app.bin"
    )));
}

#[test]
fn flash_completion_keeps_absolute_root_single_slash() {
    assert_eq!(flash_completion_parent_prefix(Path::new("/")), "/");
    assert_eq!(flash_completion_parent_prefix(Path::new("/home")), "/home/");
    assert_eq!(flash_completion_parent_prefix(Path::new(".")), "");
}

#[test]
fn flash_input_path_like_detects_typed_paths() {
    assert!(flash_input_is_path_like("build/app.bin"));
    assert!(flash_input_is_path_like("/tmp/app.hex"));
    assert!(flash_input_is_path_like("./merged.uf2"));
    assert!(!flash_input_is_path_like("merged"));
}

#[test]
fn output_line_state_default_starts_at_line_start() {
    let mut state = OutputLineState::default();
    assert!(*state.at_line_start_mut(Source::Serial));
    assert!(*state.at_line_start_mut(Source::Rtt));
    assert!(*state.at_line_start_mut(Source::Tx));
}

#[test]
fn rtt_connected_status_matches_only_success_lines() {
    assert!(is_serial_connected_status("connected"));
    assert!(is_serial_connected_status(
        "TCP serial connected 127.0.0.1:2000"
    ));
    assert!(!is_serial_connected_status("opening /dev/tty.usbmodem101"));
    assert!(!is_serial_connected_status("failed to open"));
    assert!(is_rtt_connected_status("connected up=0 down=0"));
    assert!(is_rtt_connected_status(
        "RTT stream connected 127.0.0.1:19021"
    ));
    assert!(!is_rtt_connected_status("connecting to nRF9151_xxCA"));
    assert!(!is_rtt_connected_status("failed to connect"));
    assert!(is_disconnected_status("disconnected"));
    assert!(!is_disconnected_status("not disconnected"));
}

#[test]
fn discover_control_socket_prefers_explicit_path() {
    let explicit = Path::new("/tmp/custom-rttio.sock");
    assert_eq!(discover_control_socket(Some(explicit)).unwrap(), explicit);
}

#[test]
fn discover_control_socket_walks_parent_dirs() {
    let root = std::env::temp_dir().join(format!(
        "rttio-test-{}-{}",
        std::process::id(),
        unique_test_id()
    ));
    let nested = root.join("a/b/c");
    fs::create_dir_all(&nested).unwrap();
    let socket = root.join(DEFAULT_CONTROL_SOCKET);
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();

    assert_eq!(discover_control_socket_from(nested).unwrap(), socket);

    drop(listener);
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn discover_control_socket_skips_non_socket_files() {
    let root = std::env::temp_dir().join(format!(
        "rttio-test-{}-{}",
        std::process::id(),
        unique_test_id()
    ));
    let nested = root.join("a/b/c");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join(DEFAULT_CONTROL_SOCKET), b"not a socket").unwrap();
    let socket = root.join(DEFAULT_CONTROL_SOCKET);
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();

    assert_eq!(discover_control_socket_from(nested).unwrap(), socket);

    drop(listener);
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn validate_control_socket_parent_requires_directory() {
    let root = std::env::temp_dir().join(format!(
        "rttio-test-{}-{}",
        std::process::id(),
        unique_test_id()
    ));
    fs::create_dir_all(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();

    validate_control_socket_parent(&root.join("sock")).unwrap();

    let missing = root.join("missing/sock");
    let err = validate_control_socket_parent(&missing)
        .unwrap_err()
        .to_string();
    assert!(err.contains("failed to stat control socket directory"));

    let file_parent = root.join("not-dir");
    fs::write(&file_parent, b"not a directory").unwrap();
    let err = validate_control_socket_parent(&file_parent.join("sock"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("is not a directory"));

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn validate_control_socket_parent_rejects_group_or_world_writable_directory() {
    let root = std::env::temp_dir().join(format!(
        "rttio-test-{}-{}",
        std::process::id(),
        unique_test_id()
    ));
    fs::create_dir_all(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o722)).unwrap();

    let err = validate_control_socket_parent(&root.join("sock"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("writable by group or others"));

    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn harden_control_socket_sets_owner_only_permissions() {
    let root = std::env::temp_dir().join(format!(
        "rttio-test-{}-{}",
        std::process::id(),
        unique_test_id()
    ));
    fs::create_dir_all(&root).unwrap();
    let socket = root.join(DEFAULT_CONTROL_SOCKET);
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();

    harden_control_socket(&socket).unwrap();
    let mode = fs::symlink_metadata(&socket).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);

    drop(listener);
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn validate_control_socket_for_client_accepts_hardened_socket() {
    let root = std::env::temp_dir().join(format!(
        "rttio-test-{}-{}",
        std::process::id(),
        unique_test_id()
    ));
    fs::create_dir_all(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let socket = root.join(DEFAULT_CONTROL_SOCKET);
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o600)).unwrap();

    validate_control_socket_for_client(&socket).unwrap();

    drop(listener);
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn validate_control_socket_for_client_rejects_insecure_socket_permissions() {
    let root = std::env::temp_dir().join(format!(
        "rttio-test-{}-{}",
        std::process::id(),
        unique_test_id()
    ));
    fs::create_dir_all(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let socket = root.join(DEFAULT_CONTROL_SOCKET);
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o660)).unwrap();

    let err = validate_control_socket_for_client(&socket)
        .unwrap_err()
        .to_string();
    assert!(err.contains("insecure permissions"));

    drop(listener);
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn parse_control_flash_args_accepts_quoted_path() {
    let (path, addr) = parse_control_flash_args("\"build dir/app.hex\" 0x1000").unwrap();
    assert_eq!(path, PathBuf::from("build dir/app.hex"));
    assert_eq!(addr.as_deref(), Some("0x1000"));

    let (path, addr) = parse_control_flash_args("'build/app\"x.hex' 0x2000").unwrap();
    assert_eq!(path, PathBuf::from("build/app\"x.hex"));
    assert_eq!(addr.as_deref(), Some("0x2000"));
}

#[test]
fn parse_control_flash_action_args_rejects_unknown_flag_before_file() {
    let err = parse_control_flash_action_args("--json --bad app.hex")
        .unwrap_err()
        .to_string();
    assert!(err.contains("unknown flash option"));
}

#[test]
fn parse_control_quit_args_accepts_only_json() {
    assert!(!parse_control_quit_args("quit").unwrap().unwrap());
    assert!(parse_control_quit_args("quit --json").unwrap().unwrap());
    assert!(parse_control_quit_args("quitnow").is_none());

    let err = parse_control_quit_args("quit --json --timeout 1")
        .unwrap()
        .unwrap_err()
        .to_string();
    assert!(err.contains("unknown quit option"));
}

#[test]
fn control_error_code_classifies_timeout_spelling() {
    assert_eq!(control_error_code("control client idle timeout"), "timeout");
    assert_eq!(control_error_code("transport timed out"), "timeout");
}

#[test]
fn encode_hex_renders_lowercase_without_separators() {
    assert_eq!(encode_hex(b"AT\r\n"), "41540d0a");
}

#[test]
fn decode_control_utf8_returns_none_for_binary_data() {
    assert_eq!(decode_control_utf8(b"AT\r\n").as_deref(), Some("AT\r\n"));
    assert!(decode_control_utf8(&[0xff, b'A']).is_none());
}

#[test]
fn control_history_snapshots_by_source_and_since() {
    let mut history = ControlHistory::new(1024);
    assert_eq!(history.push(Source::Serial, b"one".to_vec()), 1);
    assert_eq!(history.push(Source::Rtt, b"two".to_vec()), 4);
    assert_eq!(history.push(Source::Serial, b"three".to_vec()), 7);

    let serial = history.snapshot(ControlSource::Serial, Some(4));
    assert_eq!(serial.data, b"three");
    assert_eq!(serial.next_seq, 12);
    assert_eq!(serial.dropped_before, 1);

    let rtt = history.snapshot(ControlSource::Rtt, None);
    assert_eq!(rtt.next_seq, 12);
    assert_eq!(rtt.dropped_before, 4);

    let any = history.snapshot(ControlSource::Any, Some(1));
    assert_eq!(any.data, b"onetwothree");
    assert_eq!(any.dropped_before, 1);

    let partial = history.snapshot(ControlSource::Serial, Some(2));
    assert_eq!(partial.data, b"nethree");
    assert_eq!(partial.data_seq, 2);
}

#[test]
fn control_history_trims_old_entries() {
    let mut history = ControlHistory::new(5);
    history.push(Source::Serial, b"1234".to_vec());
    history.push(Source::Serial, b"5678".to_vec());

    let snapshot = history.snapshot(ControlSource::Serial, Some(1));
    assert_eq!(snapshot.dropped_before, 5);
    assert_eq!(snapshot.data, b"5678");
}

#[test]
fn parse_control_read_response_next_seq_finds_cursor() {
    assert_eq!(
            parse_control_read_response_next_seq(
                "OK read 4 bytes cursor_unit byte next_seq 9 dropped_before 2 complete true matched_until_hex false limited false timed_out false\nabcd\n"
            ),
            Some(9)
        );
}

#[test]
fn control_raw_read_response_serializes_json() {
    let response = ControlRawReadResponse {
        ok: true,
        source: "rtt",
        bytes: 2,
        hex: Some("abcd".to_string()),
        text: None,
        text_lossy: Some(String::from_utf8_lossy(&[0xab, 0xcd]).into_owned()),
        cursor_unit: CONTROL_CURSOR_UNIT,
        next_seq: 7,
        dropped_before: 3,
        complete: true,
        matched_until_hex: false,
        limited: false,
        timed_out: true,
    };
    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"source\":\"rtt\""));
    assert!(json.contains("\"text\":null"));
    assert!(json.contains("\"text_lossy\""));
    assert!(json.contains("\"cursor_unit\":\"byte\""));
    assert!(json.contains("\"next_seq\":7"));
    assert!(json.contains("\"complete\":true"));
    assert!(json.contains("\"matched_until_hex\":false"));
    assert!(json.contains("\"limited\":false"));
    assert!(json.contains("\"timed_out\":true"));
}

#[test]
fn write_control_client_output_flushes_each_chunk() {
    #[derive(Default)]
    struct FlushCountingWriter {
        data: Vec<u8>,
        flushes: usize,
    }

    impl Write for FlushCountingWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.data.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            self.flushes += 1;
            Ok(())
        }
    }

    let mut writer = FlushCountingWriter::default();

    write_control_client_output(&mut writer, b"OK one\n").unwrap();
    write_control_client_output(&mut writer, b"partial").unwrap();

    assert_eq!(writer.data, b"OK one\npartial");
    assert_eq!(writer.flushes, 2);
}

#[test]
fn control_response_error_requires_err_token() {
    assert_eq!(
        control_response_error("ERR bad command\n").as_deref(),
        Some("ERR bad command")
    );
    assert_eq!(control_response_error("ERR\n").as_deref(), Some("ERR"));
    assert_eq!(control_response_error("ERROR bad command\n"), None);
}

#[test]
fn control_response_error_handles_json_error() {
    assert_eq!(
        control_response_error(r#"{"ok":false,"error":"bad json command"}"#).as_deref(),
        Some("bad json command")
    );
    assert_eq!(control_response_error(r#"{"ok":true}"#), None);
}

#[test]
fn control_response_success_accepts_explicit_ok() {
    assert!(control_response_is_success("OK\n"));
    assert!(control_response_is_success("OK follow\n"));
    assert!(control_response_is_success(r#"{"ok":true}"#));
}

#[test]
fn control_response_success_rejects_unknown_lines() {
    assert!(!control_response_is_success("hello\n"));
    assert!(!control_response_is_success("ERROR bad command\n"));
    assert!(!control_response_is_success(r#"{"status":"ok"}"#));
}

#[test]
fn control_error_code_classifies_transport_lifecycle_errors() {
    assert_eq!(
        control_error_code("serial transport dropped write response"),
        "not_running"
    );
    assert_eq!(
        control_error_code("no selected transport is running"),
        "not_running"
    );
    assert_eq!(control_error_code("no transport is running"), "not_running");
    assert_eq!(
        control_error_code("rtt transport is reconnecting"),
        "not_running"
    );
    assert_eq!(
        control_error_code("RTT stream transport is reconnecting"),
        "not_running"
    );
}

#[test]
fn control_error_code_classifies_flash_path_errors() {
    assert_eq!(
        control_error_code("cannot resolve flash path missing.hex"),
        "invalid_path"
    );
    assert_eq!(
        control_error_code("cannot quote flash path containing both single and double quotes"),
        "invalid_path"
    );
    assert_eq!(
        control_error_code("app.txt is not a supported .hex/.elf/.bin/.uf2 file"),
        "invalid_path"
    );
}

#[test]
fn ctl_command_response_timeout_tracks_command_timeout() {
    assert_eq!(ctl_command_response_timeout(&CtlCommand::Follow), None);

    let timeout = ctl_command_response_timeout(&CtlCommand::Request {
        target: CtlTargetArg::Serial,
        timeout: 750,
        since: None,
        until_hex: None,
        max_bytes: None,
        fail_on_timeout: false,
        raw_hex: false,
        raw_text: false,
        hex: false,
        json: true,
        text: vec!["AT".to_string()],
    })
    .unwrap();

    assert_eq!(
        timeout,
        Duration::from_millis(
            750 + CONTROL_WRITE_ACK_TIMEOUT_MS + CONTROL_CLIENT_RESPONSE_GRACE_MS
        )
    );
}

#[test]
fn ctl_command_to_wire_builds_hex_request() {
    let command = CtlCommand::Request {
        target: CtlTargetArg::Rtt,
        timeout: 750,
        since: Some("9".to_string()),
        until_hex: Some("0d0a".to_string()),
        max_bytes: Some(64),
        fail_on_timeout: true,
        raw_hex: false,
        raw_text: false,
        hex: true,
        json: true,
        text: vec!["41".to_string(), "54".to_string()],
    };

    assert_eq!(
            ctl_command_to_wire(&command).unwrap(),
            "request --target rtt --timeout 750 --since 9 --until-hex 0d0a --max-bytes 64 --fail-on-timeout --hex --json -- 41 54"
        );
}

#[test]
fn ctl_command_to_wire_builds_since_now() {
    assert_eq!(
        ctl_command_to_wire(&CtlCommand::Read {
            timeout: 200,
            since: Some("now".to_string()),
            until_hex: None,
            max_bytes: None,
            fail_on_timeout: false,
            raw_hex: false,
            raw_text: false,
            json: true,
        })
        .unwrap(),
        "read --timeout 200 --since now --json"
    );

    assert_eq!(
        ctl_command_to_wire(&CtlCommand::Request {
            target: CtlTargetArg::Serial,
            timeout: 500,
            since: Some("now".to_string()),
            until_hex: None,
            max_bytes: None,
            fail_on_timeout: false,
            raw_hex: false,
            raw_text: false,
            hex: false,
            json: true,
            text: vec!["AT".to_string()],
        })
        .unwrap(),
        "request --target serial --timeout 500 --since now --json -- AT"
    );
}

#[test]
fn ctl_command_to_wire_rejects_invalid_hex_args() {
    let err = ctl_command_to_wire(&CtlCommand::Read {
        timeout: 200,
        since: None,
        until_hex: Some("zz".to_string()),
        max_bytes: None,
        fail_on_timeout: false,
        raw_hex: false,
        raw_text: false,
        json: true,
    })
    .unwrap_err()
    .to_string();
    assert!(err.contains("invalid --until-hex"));

    let err = ctl_command_to_wire(&CtlCommand::Write {
        target: CtlTargetArg::Rtt,
        json: true,
        hex: true,
        timeout: 200,
        text: vec!["4".to_string()],
    })
    .unwrap_err()
    .to_string();
    assert!(err.contains("invalid hex payload"));

    let err = ctl_command_to_wire(&CtlCommand::Request {
        target: CtlTargetArg::Rtt,
        timeout: 200,
        since: None,
        until_hex: Some("0d0".to_string()),
        max_bytes: None,
        fail_on_timeout: false,
        raw_hex: false,
        raw_text: false,
        hex: true,
        json: true,
        text: vec!["41".to_string()],
    })
    .unwrap_err()
    .to_string();
    assert!(err.contains("invalid --until-hex"));
}

#[test]
fn ctl_command_to_wire_builds_json_writes() {
    assert_eq!(
        ctl_command_to_wire(&CtlCommand::Write {
            target: CtlTargetArg::Serial,
            json: true,
            hex: false,
            timeout: 123,
            text: vec!["AT".to_string()],
        })
        .unwrap(),
        "write --target serial --timeout 123 --json -- AT"
    );
    assert_eq!(
        ctl_command_to_wire(&CtlCommand::Writeln {
            target: CtlTargetArg::Rtt,
            json: true,
            timeout: 234,
            text: vec!["AT".to_string()],
        })
        .unwrap(),
        "writeln --target rtt --timeout 234 --json -- AT"
    );
}

#[test]
fn ctl_command_to_wire_protects_flag_like_payload() {
    assert_eq!(
        ctl_command_to_wire(&CtlCommand::Write {
            target: CtlTargetArg::Serial,
            json: true,
            hex: false,
            timeout: 123,
            text: vec!["--literal".to_string()],
        })
        .unwrap(),
        "write --target serial --timeout 123 --json -- --literal"
    );
    assert_eq!(
        ctl_command_to_wire(&CtlCommand::Request {
            target: CtlTargetArg::Serial,
            timeout: 500,
            since: None,
            until_hex: None,
            max_bytes: None,
            fail_on_timeout: false,
            raw_hex: false,
            raw_text: false,
            hex: false,
            json: true,
            text: vec!["--literal".to_string()],
        })
        .unwrap(),
        "request --target serial --timeout 500 --json -- --literal"
    );
}

#[test]
fn ctl_command_to_wire_rejects_line_breaks_in_payload() {
    let err = ctl_command_to_wire(&CtlCommand::Write {
        target: CtlTargetArg::Serial,
        json: true,
        hex: false,
        timeout: 123,
        text: vec!["AT\nstatus".to_string()],
    })
    .unwrap_err()
    .to_string();
    assert!(err.contains("cannot contain CR or LF"));

    let err = ctl_command_to_wire(&CtlCommand::Request {
        target: CtlTargetArg::Rtt,
        timeout: 500,
        since: None,
        until_hex: None,
        max_bytes: None,
        fail_on_timeout: false,
        raw_hex: false,
        raw_text: false,
        hex: false,
        json: true,
        text: vec!["AT\r".to_string()],
    })
    .unwrap_err()
    .to_string();
    assert!(err.contains("use write/request --hex"));
}

#[test]
fn ctl_command_to_wire_builds_json_actions() {
    let root = std::env::temp_dir().join(format!(
        "rttio-test-{}-{}",
        std::process::id(),
        unique_test_id()
    ));
    fs::create_dir_all(&root).unwrap();
    let flash_file = root.join("app.hex");
    fs::write(&flash_file, b":00000001FF\n").unwrap();

    assert_eq!(
        ctl_command_to_wire(&CtlCommand::Reset {
            json: true,
            timeout: 123,
        })
        .unwrap(),
        "reset --json --timeout 123"
    );
    assert_eq!(
        ctl_command_to_wire(&CtlCommand::Reconnect {
            json: true,
            timeout: 234,
        })
        .unwrap(),
        "reconnect --json --timeout 234"
    );
    assert_eq!(
        ctl_command_to_wire(&CtlCommand::Erase {
            json: true,
            timeout: 345,
        })
        .unwrap(),
        "erase --json --timeout 345"
    );
    assert_eq!(
        ctl_command_to_wire(&CtlCommand::Flash {
            json: true,
            file: flash_file.clone(),
            addr: 0x1000,
            timeout: 456,
        })
        .unwrap(),
        format!(
            "flash --json --timeout 456 \"{}\" 0x00001000",
            flash_file.canonicalize().unwrap().display()
        )
    );

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn ctl_command_to_wire_quotes_flash_path_with_single_quotes_when_needed() {
    let root = std::env::temp_dir().join(format!(
        "rttio-test-{}-{}",
        std::process::id(),
        unique_test_id()
    ));
    fs::create_dir_all(&root).unwrap();
    let flash_file = root.join("app\"x.hex");
    fs::write(&flash_file, b":00000001FF\n").unwrap();

    let command = ctl_command_to_wire(&CtlCommand::Flash {
        json: true,
        file: flash_file.clone(),
        addr: 0x1000,
        timeout: 456,
    })
    .unwrap();

    assert_eq!(
        command,
        format!(
            "flash --json --timeout 456 '{}' 0x00001000",
            flash_file.canonicalize().unwrap().display()
        )
    );
    let wire_path = format!(
        "'{}' 0x00001000",
        flash_file.canonicalize().unwrap().display()
    );
    let (path, addr) = parse_control_flash_args(&wire_path).unwrap();
    assert_eq!(path, flash_file.canonicalize().unwrap());
    assert_eq!(addr.as_deref(), Some("0x00001000"));

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn ctl_command_to_wire_rejects_unquotable_flash_path() {
    let root = std::env::temp_dir().join(format!(
        "rttio-test-{}-{}",
        std::process::id(),
        unique_test_id()
    ));
    fs::create_dir_all(&root).unwrap();
    let flash_file = root.join("app'\"x.hex");
    fs::write(&flash_file, b":00000001FF\n").unwrap();

    let err = ctl_command_to_wire(&CtlCommand::Flash {
        json: true,
        file: flash_file,
        addr: 0x1000,
        timeout: 456,
    })
    .unwrap_err()
    .to_string();

    assert!(err.contains("cannot quote flash path"));
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn ctl_command_to_wire_rejects_missing_flash_path() {
    let err = ctl_command_to_wire(&CtlCommand::Flash {
        json: true,
        file: PathBuf::from("missing.hex"),
        addr: 0,
        timeout: 456,
    })
    .unwrap_err()
    .to_string();

    assert!(err.contains("cannot resolve flash path"));
}

#[test]
fn ctl_command_to_wire_rejects_excessive_timeout() {
    let err = ctl_command_to_wire(&CtlCommand::Read {
        timeout: CONTROL_MAX_TIMEOUT_MS + 1,
        since: None,
        until_hex: None,
        max_bytes: None,
        fail_on_timeout: false,
        raw_hex: false,
        raw_text: false,
        json: true,
    })
    .unwrap_err()
    .to_string();
    assert!(err.contains("exceeds maximum"));
}

#[test]
fn control_commands_help_mentions_hex_option() {
    assert!(!CONTROL_COMMANDS_HELP.contains("request-hex"));
    assert!(!CONTROL_COMMANDS_HELP.contains("write-hex"));
    assert!(CONTROL_COMMANDS_HELP.contains("version [--json]"));
    assert!(CONTROL_COMMANDS_HELP.contains("clear-buffer [--json]"));
    assert!(CONTROL_COMMANDS_HELP
        .contains("write [--target current|serial|rtt] [--timeout ms] [--hex]"));
    assert!(CONTROL_COMMANDS_HELP.contains("request [--target ...]"));
    assert!(CONTROL_COMMANDS_HELP.contains("[--hex]"));
    assert!(CONTROL_COMMANDS_HELP.contains("--max-bytes n"));
    assert!(CONTROL_COMMANDS_HELP.contains("flash [--json]"));
    assert!(CONTROL_COMMANDS_HELP.contains("quit [--json]"));
    assert!(!CONTROL_COMMANDS_HELP.starts_with("OK "));
}

#[test]
fn ctl_command_to_wire_builds_json_commands() {
    assert_eq!(
        ctl_command_to_wire(&CtlCommand::Commands { json: true }).unwrap(),
        "commands --json"
    );
    assert_eq!(
        ctl_command_to_wire(&CtlCommand::Commands { json: false }).unwrap(),
        "commands"
    );
}

#[test]
fn ctl_command_to_wire_builds_version() {
    assert_eq!(
        ctl_command_to_wire(&CtlCommand::Version { json: true }).unwrap(),
        "version --json"
    );
    assert_eq!(
        ctl_command_to_wire(&CtlCommand::Version { json: false }).unwrap(),
        "version"
    );
}

#[test]
fn ctl_command_to_wire_builds_clear_buffer() {
    assert_eq!(
        ctl_command_to_wire(&CtlCommand::ClearBuffer { json: true }).unwrap(),
        "clear-buffer --json"
    );
    assert_eq!(
        ctl_command_to_wire(&CtlCommand::ClearBuffer { json: false }).unwrap(),
        "clear-buffer"
    );
}

#[test]
fn ctl_command_to_wire_builds_json_quit() {
    assert_eq!(
        ctl_command_to_wire(&CtlCommand::Quit { json: true }).unwrap(),
        "quit --json"
    );
    assert_eq!(
        ctl_command_to_wire(&CtlCommand::Quit { json: false }).unwrap(),
        "quit"
    );
}

#[test]
fn control_commands_json_reports_capabilities() {
    let response = collect_control_commands_json();
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["protocol"], "rttio-control");
    assert_eq!(json["version"], CONTROL_PROTOCOL_VERSION);
    assert_eq!(json["rttio_version"], RTTIO_VERSION);
    assert_eq!(json["git_hash"], RTTIO_GIT_HASH);
    assert_eq!(json["payload_separator"], "--");
    assert_eq!(json["error_fields"][0], "ok");
    assert_eq!(json["error_fields"][1], "code");
    assert_eq!(json["error_fields"][2], "error");
    assert_eq!(json["default_timeouts_ms"]["read"], 200);
    assert_eq!(json["default_timeouts_ms"]["request_read"], 500);
    assert_eq!(
        json["default_timeouts_ms"]["write"],
        CONTROL_WRITE_ACK_TIMEOUT_MS
    );
    assert_eq!(
        json["default_timeouts_ms"]["action"],
        CONTROL_ACTION_TIMEOUT_MS
    );
    assert_eq!(
        json["default_timeouts_ms"]["flash"],
        CONTROL_FLASH_TIMEOUT_MS
    );
    assert!(json["error_codes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|code| code == "timeout"));
    assert!(json["error_codes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|code| code == "invalid_argument"));
    assert_eq!(json["features"]["request_response"], true);
    assert_eq!(json["features"]["until_hex"], true);
    assert_eq!(json["features"]["complete"], true);
    assert_eq!(json["features"]["timed_out"], true);
    assert_eq!(json["features"]["fail_on_timeout"], true);
    assert_eq!(json["features"]["byte_cursor"], true);
    assert_eq!(json["features"]["bounded_reads"], true);
    assert_eq!(json["features"]["structured_jlink_results"], true);
    assert_eq!(
        json["features"]["max_read_bytes"],
        CONTROL_HISTORY_MAX_BYTES
    );
    assert_eq!(json["features"]["max_timeout_ms"], CONTROL_MAX_TIMEOUT_MS);
    assert!(json["commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|command| command["name"] == "version"
            && command["response"] == "version"
            && command["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "git_hash")));
    assert!(json["commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|command| command["name"] == "request"
            && command["payload"] == "text-or-hex-after-separator"
            && command["example"]
                .as_str()
                .unwrap()
                .starts_with("request --target serial")
            && command["options"]
                .as_array()
                .unwrap()
                .iter()
                .any(|option| option == "--hex")
            && command["options"]
                .as_array()
                .unwrap()
                .iter()
                .any(|option| option == "--raw-hex")
            && command["options"]
                .as_array()
                .unwrap()
                .iter()
                .any(|option| option == "--raw-text")
            && command["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "response.text")
            && command["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "response.cursor_unit")
            && command["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "response.limited")
            && command["options"]
                .as_array()
                .unwrap()
                .iter()
                .any(|option| option == "--max-bytes")
            && command["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "response.timed_out")));
    assert!(json["commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|command| command["name"] == "read"
            && command["options"]
                .as_array()
                .unwrap()
                .iter()
                .any(|option| option == "--until-hex")
            && command["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "cursor_unit")
            && command["options"]
                .as_array()
                .unwrap()
                .iter()
                .any(|option| option == "--max-bytes")
            && command["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "limited")
            && command["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "timed_out")));
    assert!(json["commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|command| command["name"] == "request"
            && command["options"]
                .as_array()
                .unwrap()
                .iter()
                .any(|option| option == "--fail-on-timeout")
            && command["options"]
                .as_array()
                .unwrap()
                .iter()
                .any(|option| option == "--target")
            && command["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "actual_targets")));
    assert!(json["commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|command| command["name"] == "request"
            && command["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "write_timeout_ms")
            && command["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "read_timeout_ms")));
    assert!(json["commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|command| command["name"] == "write"
            && command["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "actual_targets")));
    assert!(json["commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|command| command["name"] == "status"
            && command["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "rtt_tcp_port")
            && command["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "pid")
            && command["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "cwd")
            && command["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "control_socket")
            && command["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "history_max_bytes")
            && command["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "serial_next_seq")
            && command["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "rtt_dropped_before")));
    assert!(json["commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|command| command["name"] == "clear-buffer"
            && command["json"] == true
            && command["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "message")));
    assert!(json["commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|command| command["name"] == "quit"
            && command["json"] == true
            && command["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "message")));
    assert_eq!(json["features"]["action_timeout"], true);
    assert_eq!(json["features"]["error_codes"], true);
}

#[tokio::test]
async fn control_clear_buffer_drops_history() {
    let (input_tx, _input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    history.lock().await.push(Source::Serial, b"abc".to_vec());
    history.lock().await.push(Source::Rtt, b"de".to_vec());

    let response = handle_control_line(
        "clear-buffer --json",
        &input_tx,
        &mut output_rx,
        &mut raw_rx,
        &history,
        &test_control_state(),
        LineEnding::CrLf,
    )
    .await;
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "clear-buffer");
    assert_eq!(json["reported_result"], "5");
    assert_eq!(history.lock().await.bytes(), 0);
}

#[tokio::test]
async fn control_client_protocol_has_no_connect_banner() {
    let (client, server) = UnixStream::pair().unwrap();
    let (input_tx, _input_rx) = mpsc::channel(1);
    let (terminal_tx, _terminal_rx) = mpsc::channel(1);
    let (output_tx, output_rx) = broadcast::channel(1);
    let (_raw_tx, raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state_with_route(Route::Serial);

    let handle = tokio::spawn(handle_control_client(
        server,
        test_control_client_context(input_tx, terminal_tx, output_rx, raw_rx, history, state),
    ));

    let (reader, mut writer) = client.into_split();
    writer.write_all(b"status\n").await.unwrap();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();

    assert!(line.starts_with("OK status "));
    assert!(line.contains("protocol rttio-control"));
    drop(output_tx);
    handle.abort();
}

#[tokio::test]
async fn control_command_reader_strips_crlf() {
    let input = b"status\r\n";
    let mut reader = BufReader::new(&input[..]);

    assert_eq!(
        read_control_command_line(&mut reader).await.unwrap(),
        Some("status".to_string())
    );
}

#[tokio::test]
async fn control_command_reader_rejects_oversized_command() {
    let input = vec![b'a'; CONTROL_MAX_COMMAND_BYTES + 1];
    let mut reader = BufReader::new(&input[..]);

    let err = read_control_command_line(&mut reader)
        .await
        .unwrap_err()
        .to_string();

    assert!(err.contains("control command exceeds"));
}

#[tokio::test]
async fn control_client_idle_timeout_releases_connection() {
    let (client, server) = UnixStream::pair().unwrap();
    let (input_tx, _input_rx) = mpsc::channel(1);
    let (terminal_tx, _terminal_rx) = mpsc::channel(1);
    let (_output_tx, output_rx) = broadcast::channel(1);
    let (_raw_tx, raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state_with_route(Route::Serial);

    let handle = tokio::spawn(handle_control_client(
        server,
        test_control_client_context(input_tx, terminal_tx, output_rx, raw_rx, history, state),
    ));

    let mut reader = BufReader::new(client);
    let mut line = String::new();
    tokio::time::timeout(Duration::from_secs(1), reader.read_line(&mut line))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(line, "ERR control client idle timeout\n");
    handle.await.unwrap();
}

#[tokio::test]
async fn control_help_alias_is_not_supported() {
    let (input_tx, _input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();

    let response = handle_control_line(
        "help --json",
        &input_tx,
        &mut output_rx,
        &mut raw_rx,
        &history,
        &state,
        LineEnding::CrLf,
    )
    .await;
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["code"], "unknown_command");
    assert_eq!(json["error"], "unknown command");
}

#[tokio::test]
async fn control_commands_json_works_over_socket_handler() {
    let (input_tx, _input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();

    let response = handle_control_line(
        "commands --json",
        &input_tx,
        &mut output_rx,
        &mut raw_rx,
        &history,
        &state,
        LineEnding::CrLf,
    )
    .await;
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], true);
    assert!(json["commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|command| command["name"] == "status"));
    assert!(json["commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|command| command["name"] == "version"));
}

#[tokio::test]
async fn control_commands_option_error_returns_json_error() {
    let (input_tx, _input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();

    let response = handle_control_line(
        "commands --json --bad",
        &input_tx,
        &mut output_rx,
        &mut raw_rx,
        &history,
        &state,
        LineEnding::CrLf,
    )
    .await;
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["code"], "unknown_option");
    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("unknown commands option"));
}

#[tokio::test]
async fn control_status_json_option_error_returns_json_error() {
    let (input_tx, _input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();

    let response = handle_control_line(
        "status --json --bad",
        &input_tx,
        &mut output_rx,
        &mut raw_rx,
        &history,
        &state,
        LineEnding::CrLf,
    )
    .await;
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["code"], "unknown_option");
    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("unknown status option"));
}

#[tokio::test]
async fn control_unknown_json_command_returns_json_error() {
    let (input_tx, _input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();

    let response = handle_control_line(
        "nope --json",
        &input_tx,
        &mut output_rx,
        &mut raw_rx,
        &history,
        &state,
        LineEnding::CrLf,
    )
    .await;
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["code"], "unknown_command");
    assert_eq!(json["error"], "unknown command");
}

#[tokio::test]
async fn control_read_requires_command_boundary() {
    let (input_tx, _input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();

    let response = handle_control_line(
        "readfoo --json",
        &input_tx,
        &mut output_rx,
        &mut raw_rx,
        &history,
        &state,
        LineEnding::CrLf,
    )
    .await;
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["code"], "unknown_command");
    assert_eq!(json["error"], "unknown command");
}

#[tokio::test]
async fn control_payload_commands_report_missing_arguments() {
    let (input_tx, _input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();

    for command in ["write --json", "request --json"] {
        let response = handle_control_line(
            command,
            &input_tx,
            &mut output_rx,
            &mut raw_rx,
            &history,
            &state,
            LineEnding::CrLf,
        )
        .await;
        let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
        assert_eq!(json["ok"], false, "{command}");
        assert_eq!(json["code"], "invalid_argument", "{command}");
        assert!(
            json["error"].as_str().unwrap().contains("missing"),
            "{command}"
        );
    }

    let response = handle_control_line(
        "flash --json",
        &input_tx,
        &mut output_rx,
        &mut raw_rx,
        &history,
        &state,
        LineEnding::CrLf,
    )
    .await;
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["code"], "invalid_argument");
    assert!(json["error"].as_str().unwrap().contains("usage: flash"));
}

#[tokio::test]
async fn control_client_status_returns_after_response() {
    let (socket, listener) = bind_hardened_test_control_socket();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (input_tx, _input_rx) = mpsc::channel(1);
        let (terminal_tx, _terminal_rx) = mpsc::channel(1);
        let (_output_tx, output_rx) = broadcast::channel(1);
        let (_raw_tx, raw_rx) = broadcast::channel(1);
        let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
        let state = test_control_state();
        handle_control_client(
            stream,
            test_control_client_context(input_tx, terminal_tx, output_rx, raw_rx, history, state),
        )
        .await;
    });

    let mut output = Vec::new();
    let result = tokio::time::timeout(Duration::from_millis(500), async {
        control_client_with_output(
            &socket,
            "status",
            Some(Duration::from_millis(500)),
            |bytes| {
                output.extend_from_slice(bytes);
                Ok(())
            },
        )
        .await
    })
    .await;

    let _ = fs::remove_file(&socket);
    assert!(result.is_ok(), "control_client timed out waiting for EOF");
    result.unwrap().unwrap();
    assert!(output.starts_with(b"OK status "));
    assert!(output
        .windows(b"protocol rttio-control".len())
        .any(|window| window == b"protocol rttio-control"));
    server.await.unwrap();
}

#[tokio::test]
async fn control_client_streams_follow_bytes_without_newline() {
    let (socket, listener) = bind_hardened_test_control_socket();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);
        let mut command = String::new();
        reader.read_line(&mut command).await.unwrap();
        assert_eq!(command, "follow\n");
        writer.write_all(b"OK follow\n").await.unwrap();
        writer.write_all(b"partial").await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
    });

    let (seen_tx, mut seen_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let client_socket = socket.clone();
    let client = tokio::spawn(async move {
        control_client_with_output(&client_socket, "follow", None, |bytes| {
            seen_tx.send(bytes.to_vec()).map_err(io::Error::other)
        })
        .await
    });

    let mut output = Vec::new();
    tokio::time::timeout(Duration::from_secs(1), async {
        while !output
            .windows(b"partial".len())
            .any(|window| window == b"partial")
        {
            let chunk = seen_rx.recv().await.unwrap();
            output.extend(chunk);
        }
    })
    .await
    .unwrap();

    assert!(output.starts_with(b"OK follow\n"));
    assert!(output
        .windows(b"partial".len())
        .any(|window| window == b"partial"));
    let _ = fs::remove_file(&socket);
    server.await.unwrap();
    client.await.unwrap().unwrap();
}

#[tokio::test]
async fn control_client_times_out_waiting_for_response() {
    let (socket, listener) = bind_hardened_test_control_socket();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (reader, _writer) = stream.into_split();
        let mut reader = BufReader::new(reader);
        let mut command = String::new();
        reader.read_line(&mut command).await.unwrap();
        assert_eq!(command, "status\n");
        tokio::time::sleep(Duration::from_secs(1)).await;
    });

    let mut output = Vec::new();
    let err = control_client_with_output(
        &socket,
        "status",
        Some(Duration::from_millis(50)),
        |bytes| {
            output.extend_from_slice(bytes);
            Ok(())
        },
    )
    .await
    .unwrap_err();

    let _ = fs::remove_file(&socket);
    assert!(err.to_string().contains("control response timed out"));
    assert!(output.is_empty());
    server.abort();
}

#[tokio::test]
async fn control_client_errors_on_empty_response() {
    let (socket, listener) = bind_hardened_test_control_socket();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (reader, _writer) = stream.into_split();
        let mut reader = BufReader::new(reader);
        let mut command = String::new();
        reader.read_line(&mut command).await.unwrap();
        assert_eq!(command, "status\n");
    });

    let mut output = Vec::new();
    let err = control_client_with_output(
        &socket,
        "status",
        Some(Duration::from_millis(500)),
        |bytes| {
            output.extend_from_slice(bytes);
            Ok(())
        },
    )
    .await
    .unwrap_err();

    let _ = fs::remove_file(&socket);
    assert!(err
        .to_string()
        .contains("control socket closed without response"));
    assert!(output.is_empty());
    server.await.unwrap();
}

#[tokio::test]
async fn control_client_returns_immediately_on_error_line() {
    let (socket, listener) = bind_hardened_test_control_socket();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);
        let mut command = String::new();
        reader.read_line(&mut command).await.unwrap();
        assert_eq!(command, "status\n");
        writer.write_all(b"ERR bad command\n").await.unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
    });

    let mut output = Vec::new();
    let result = tokio::time::timeout(Duration::from_millis(200), async {
        control_client_with_output(&socket, "status", Some(Duration::from_secs(5)), |bytes| {
            output.extend_from_slice(bytes);
            Ok(())
        })
        .await
    })
    .await;

    let _ = fs::remove_file(&socket);
    let err = result
        .expect("client should return before the outer timeout")
        .unwrap_err();
    assert!(err.to_string().contains("ERR bad command"));
    assert_eq!(output, b"ERR bad command\n");
    server.abort();
}

#[tokio::test]
async fn control_client_rejects_unknown_first_line() {
    let (socket, listener) = bind_hardened_test_control_socket();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);
        let mut command = String::new();
        reader.read_line(&mut command).await.unwrap();
        assert_eq!(command, "status\n");
        writer.write_all(b"hello\n").await.unwrap();
    });

    let mut output = Vec::new();
    let err = control_client_with_output(
        &socket,
        "status",
        Some(Duration::from_millis(500)),
        |bytes| {
            output.extend_from_slice(bytes);
            Ok(())
        },
    )
    .await
    .unwrap_err();

    let _ = fs::remove_file(&socket);
    assert!(err.to_string().contains("invalid control response: hello"));
    assert_eq!(output, b"hello\n");
    server.await.unwrap();
}

#[tokio::test]
async fn control_server_rejects_clients_over_limit() {
    let socket = std::env::temp_dir().join(format!(
        "rttio-test-{}-{}.sock",
        std::process::id(),
        unique_test_id()
    ));
    let (input_tx, _input_rx) = mpsc::channel(1);
    let (terminal_tx, _terminal_rx) = mpsc::channel(1);
    let (output_tx, _output_rx) = broadcast::channel(1);
    let (raw_tx, _raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();
    let server = tokio::spawn(control_server(
        input_tx,
        ControlServerContext {
            path: socket.clone(),
            terminal_tx,
            output_tx,
            raw_tx,
            history,
            state,
            line_ending: LineEnding::CrLf,
        },
    ));

    let mut first = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            match UnixStream::connect(&socket).await {
                Ok(stream) => break stream,
                Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }
    })
    .await
    .unwrap();
    first.write_all(b"follow\n").await.unwrap();
    let mut first_reader = BufReader::new(first);
    let mut line = String::new();
    first_reader.read_line(&mut line).await.unwrap();
    assert_eq!(line, "OK follow\n");

    let mut second = UnixStream::connect(&socket).await.unwrap();
    let mut second_reader = BufReader::new(&mut second);
    let mut error = String::new();
    second_reader.read_line(&mut error).await.unwrap();

    assert_eq!(error, "ERR control client limit reached\n");
    server.abort();
    let _ = fs::remove_file(&socket);
}

#[tokio::test]
async fn control_client_returns_error_on_err_response() {
    let (socket, listener) = bind_hardened_test_control_socket();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        stream.write_all(b"ERR bad command\n").await.unwrap();
    });

    let mut output = Vec::new();
    let err = control_client_with_output(
        &socket,
        "status",
        Some(Duration::from_millis(500)),
        |bytes| {
            output.extend_from_slice(bytes);
            Ok(())
        },
    )
    .await
    .unwrap_err();

    let _ = fs::remove_file(&socket);
    assert!(err.to_string().contains("ERR bad command"));
    assert_eq!(output, b"ERR bad command\n");
    server.await.unwrap();
}

#[tokio::test]
async fn control_client_returns_error_on_json_error_response() {
    let (socket, listener) = bind_hardened_test_control_socket();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        stream
            .write_all(br#"{"ok":false,"error":"bad json command"}"#)
            .await
            .unwrap();
        stream.write_all(b"\n").await.unwrap();
    });

    let mut output = Vec::new();
    let err = control_client_with_output(
        &socket,
        "status --json",
        Some(Duration::from_millis(500)),
        |bytes| {
            output.extend_from_slice(bytes);
            Ok(())
        },
    )
    .await
    .unwrap_err();

    let _ = fs::remove_file(&socket);
    assert!(err.to_string().contains("bad json command"));
    assert_eq!(
        output,
        br#"{"ok":false,"error":"bad json command"}
"#
    );
    server.await.unwrap();
}

#[tokio::test]
async fn control_request_hex_writes_raw_bytes_without_line_ending() {
    let (input_tx, mut input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();

    let request = tokio::spawn({
        let history = Arc::clone(&history);
        let state = Arc::clone(&state);
        async move {
            handle_control_line(
                "request --hex --target serial --timeout 0 41 54",
                &input_tx,
                &mut output_rx,
                &mut raw_rx,
                &history,
                &state,
                LineEnding::CrLf,
            )
            .await
        }
    });

    match input_rx.recv().await.unwrap() {
        InputEvent::Control(ControlRequest::Write {
            target,
            bytes,
            timeout,
            reply,
        }) => {
            assert_eq!(target, ControlTarget::Serial);
            assert_eq!(bytes, b"AT");
            assert_eq!(timeout, Duration::from_millis(CONTROL_WRITE_ACK_TIMEOUT_MS));
            reply.send("OK write 2 bytes\n".to_string()).unwrap();
        }
        other => panic!("unexpected input event: {other:?}"),
    }

    let response = request.await.unwrap();
    assert!(response.starts_with("OK read 0 bytes"));
    assert!(response.contains("matched_until_hex false"));
    assert!(response.contains("limited false"));
    assert!(response.contains("timed_out true"));
}

#[tokio::test]
async fn control_request_json_wraps_written_metadata_and_response() {
    let (input_tx, mut input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state_with_route(Route::Serial);

    let request = tokio::spawn({
        let history = Arc::clone(&history);
        let state = Arc::clone(&state);
        async move {
            handle_control_line(
                "request --target serial --timeout 50 --until-hex 0d0a --raw-hex --json AT",
                &input_tx,
                &mut output_rx,
                &mut raw_rx,
                &history,
                &state,
                LineEnding::CrLf,
            )
            .await
        }
    });

    match input_rx.recv().await.unwrap() {
        InputEvent::Control(ControlRequest::Write {
            target,
            bytes,
            timeout,
            reply,
        }) => {
            assert_eq!(target, ControlTarget::Serial);
            assert_eq!(bytes, b"AT\r\n");
            assert_eq!(timeout, Duration::from_millis(CONTROL_WRITE_ACK_TIMEOUT_MS));
            reply.send("OK serial write 4 bytes\n".to_string()).unwrap();
            let data = b"OK\r\n".to_vec();
            let seq = history.lock().await.push(Source::Serial, data.clone());
            raw_tx
                .send(ControlOutput {
                    seq,
                    source: Source::Serial,
                    data,
                })
                .unwrap();
        }
        other => panic!("unexpected input event: {other:?}"),
    }

    let response = request.await.unwrap();
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "request");
    assert_eq!(json["target"], "serial");
    assert_eq!(json["actual_targets"], serde_json::json!(["serial"]));
    assert_eq!(json["written_bytes"], 4);
    assert_eq!(json["write_timeout_ms"], CONTROL_WRITE_ACK_TIMEOUT_MS);
    assert_eq!(json["read_timeout_ms"], 50);
    assert_eq!(json["response"]["source"], "serial");
    assert_eq!(json["response"]["bytes"], 4);
    assert_eq!(json["response"]["hex"], "4f4b0d0a");
    assert_eq!(json["response"]["cursor_unit"], CONTROL_CURSOR_UNIT);
    assert_eq!(json["response"]["matched_until_hex"], true);
    assert_eq!(json["response"]["timed_out"], false);
}

#[tokio::test]
async fn control_request_json_fail_on_timeout_returns_timeout_error() {
    let (input_tx, mut input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();

    let request = tokio::spawn({
        let history = Arc::clone(&history);
        let state = Arc::clone(&state);
        async move {
            handle_control_line(
                "request --target serial --timeout 0 --until-hex 0d0a --fail-on-timeout --json AT",
                &input_tx,
                &mut output_rx,
                &mut raw_rx,
                &history,
                &state,
                LineEnding::CrLf,
            )
            .await
        }
    });

    match input_rx.recv().await.unwrap() {
        InputEvent::Control(ControlRequest::Write { reply, .. }) => {
            reply.send("OK serial write 4 bytes\n".to_string()).unwrap();
        }
        other => panic!("unexpected input event: {other:?}"),
    }

    let response = request.await.unwrap();
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["code"], "timeout");
    assert!(json["error"].as_str().unwrap().contains("until-hex"));
}

#[tokio::test]
async fn control_request_json_without_transport_returns_not_running_error() {
    let (input_tx, mut input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();

    let request = tokio::spawn({
        let history = Arc::clone(&history);
        let state = Arc::clone(&state);
        async move {
            handle_control_line(
                "request --target serial --timeout 50 --json AT",
                &input_tx,
                &mut output_rx,
                &mut raw_rx,
                &history,
                &state,
                LineEnding::CrLf,
            )
            .await
        }
    });

    match input_rx.recv().await.unwrap() {
        InputEvent::Control(ControlRequest::Write {
            target,
            bytes,
            reply,
            ..
        }) => {
            assert_eq!(target, ControlTarget::Serial);
            assert_eq!(bytes, b"AT\r\n");
            handle_control_request(
                ControlRequest::Write {
                    target,
                    bytes,
                    timeout: Duration::from_millis(CONTROL_WRITE_ACK_TIMEOUT_MS),
                    reply,
                },
                Route::Serial,
                &None,
                &None,
            )
            .await;
        }
        other => panic!("unexpected input event: {other:?}"),
    }

    let response = request.await.unwrap();
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["code"], "not_running");
    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("serial transport is not running"));
}

#[tokio::test]
async fn control_request_hex_json_without_transport_returns_not_running_error() {
    let (input_tx, mut input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();

    let request = tokio::spawn({
        let history = Arc::clone(&history);
        let state = Arc::clone(&state);
        async move {
            handle_control_line(
                "request --hex --target rtt --timeout 50 --json 41",
                &input_tx,
                &mut output_rx,
                &mut raw_rx,
                &history,
                &state,
                LineEnding::CrLf,
            )
            .await
        }
    });

    match input_rx.recv().await.unwrap() {
        InputEvent::Control(ControlRequest::Write {
            target,
            bytes,
            reply,
            ..
        }) => {
            assert_eq!(target, ControlTarget::Rtt);
            assert_eq!(bytes, b"A");
            handle_control_request(
                ControlRequest::Write {
                    target,
                    bytes,
                    timeout: Duration::from_millis(CONTROL_WRITE_ACK_TIMEOUT_MS),
                    reply,
                },
                Route::Rtt,
                &None,
                &None,
            )
            .await;
        }
        other => panic!("unexpected input event: {other:?}"),
    }

    let response = request.await.unwrap();
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["code"], "not_running");
    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("rtt transport is not running"));
}

#[tokio::test]
async fn control_request_text_without_transport_returns_write_error() {
    let (input_tx, mut input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();

    let request = tokio::spawn({
        let history = Arc::clone(&history);
        let state = Arc::clone(&state);
        async move {
            handle_control_line(
                "request --target serial --timeout 50 --json AT",
                &input_tx,
                &mut output_rx,
                &mut raw_rx,
                &history,
                &state,
                LineEnding::CrLf,
            )
            .await
        }
    });

    match input_rx.recv().await.unwrap() {
        InputEvent::Control(ControlRequest::Write {
            target,
            bytes,
            reply,
            ..
        }) => {
            assert_eq!(target, ControlTarget::Serial);
            assert_eq!(bytes, b"AT\r\n");
            handle_control_request(
                ControlRequest::Write {
                    target,
                    bytes,
                    timeout: Duration::from_millis(CONTROL_WRITE_ACK_TIMEOUT_MS),
                    reply,
                },
                Route::Serial,
                &None,
                &None,
            )
            .await;
        }
        other => panic!("unexpected input event: {other:?}"),
    }

    let response = request.await.unwrap();
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["code"], "not_running");
    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("serial transport is not running"));
}

#[tokio::test]
async fn control_read_json_stops_after_until_hex_match() {
    let (input_tx, _input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state_with_route(Route::Serial);

    let request = tokio::spawn({
        let history = Arc::clone(&history);
        let state = Arc::clone(&state);
        async move {
            handle_control_line(
                "read --timeout 100 --since 0 --until-hex 0d0a --raw-hex --raw-text --json",
                &input_tx,
                &mut output_rx,
                &mut raw_rx,
                &history,
                &state,
                LineEnding::CrLf,
            )
            .await
        }
    });

    let data = b"OK\r\nignored".to_vec();
    let seq = history.lock().await.push(Source::Serial, data.clone());
    raw_tx
        .send(ControlOutput {
            seq,
            source: Source::Serial,
            data,
        })
        .unwrap();

    let response = request.await.unwrap();
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["source"], "serial");
    assert_eq!(json["hex"], "4f4b0d0a");
    assert_eq!(json["text"], "OK\r\n");
    assert_eq!(json["text_lossy"], "OK\r\n");
    assert_eq!(json["cursor_unit"], CONTROL_CURSOR_UNIT);
    assert_eq!(json["complete"], true);
    assert_eq!(json["matched_until_hex"], true);
    assert_eq!(json["timed_out"], false);
}

#[tokio::test]
async fn control_read_until_hex_returns_cursor_after_delimiter_inside_chunk() {
    let (input_tx, _input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();
    let data = b"OK\r\nNEXT\r\n".to_vec();
    let seq = history.lock().await.push(Source::Serial, data.clone());
    raw_tx
        .send(ControlOutput {
            seq,
            source: Source::Serial,
            data,
        })
        .unwrap();

    let response = handle_control_line(
        "read --timeout 0 --since 0 --until-hex 0d0a --raw-hex --json",
        &input_tx,
        &mut output_rx,
        &mut raw_rx,
        &history,
        &state,
        LineEnding::CrLf,
    )
    .await;
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["hex"], "4f4b0d0a");
    assert_eq!(json["cursor_unit"], CONTROL_CURSOR_UNIT);
    assert_eq!(json["next_seq"], 5);
    assert_eq!(json["matched_until_hex"], true);

    let response = handle_control_line(
        "read --timeout 0 --since 5 --until-hex 0d0a --raw-hex --json",
        &input_tx,
        &mut output_rx,
        &mut raw_rx,
        &history,
        &state,
        LineEnding::CrLf,
    )
    .await;
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["hex"], "4e4558540d0a");
    assert_eq!(json["cursor_unit"], CONTROL_CURSOR_UNIT);
    assert_eq!(json["next_seq"], 11);
    assert_eq!(json["matched_until_hex"], true);
}

#[tokio::test]
async fn control_read_json_reports_timeout_without_until_match() {
    let (input_tx, _input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();

    let response = handle_control_line(
        "read --timeout 0 --until-hex 0d0a --json",
        &input_tx,
        &mut output_rx,
        &mut raw_rx,
        &history,
        &state,
        LineEnding::CrLf,
    )
    .await;

    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], true);
    assert!(json.get("hex").is_none());
    assert!(json.get("text_lossy").is_none());
    assert_eq!(json["matched_until_hex"], false);
    assert_eq!(json["timed_out"], true);
}

#[tokio::test]
async fn control_read_json_respects_max_bytes_and_cursor() {
    let (input_tx, _input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();
    let data = b"abcdef".to_vec();
    let seq = history.lock().await.push(Source::Serial, data.clone());
    raw_tx
        .send(ControlOutput {
            seq,
            source: Source::Serial,
            data,
        })
        .unwrap();

    let response = handle_control_line(
        "read --timeout 0 --since 0 --max-bytes 3 --json",
        &input_tx,
        &mut output_rx,
        &mut raw_rx,
        &history,
        &state,
        LineEnding::CrLf,
    )
    .await;
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["text"], "abc");
    assert_eq!(json["next_seq"], 4);
    assert_eq!(json["limited"], true);
    assert_eq!(json["timed_out"], false);

    let response = handle_control_line(
        "read --timeout 0 --since 4 --max-bytes 3 --json",
        &input_tx,
        &mut output_rx,
        &mut raw_rx,
        &history,
        &state,
        LineEnding::CrLf,
    )
    .await;
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["text"], "def");
    assert_eq!(json["next_seq"], 7);
    assert_eq!(json["limited"], true);
}

#[tokio::test]
async fn control_read_json_reports_null_text_for_invalid_utf8() {
    let (input_tx, _input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();
    let data = vec![0xff, b'A'];
    let seq = history.lock().await.push(Source::Serial, data.clone());
    raw_tx
        .send(ControlOutput {
            seq,
            source: Source::Serial,
            data,
        })
        .unwrap();

    let response = handle_control_line(
        "read --timeout 0 --since 0 --raw-hex --raw-text --json",
        &input_tx,
        &mut output_rx,
        &mut raw_rx,
        &history,
        &state,
        LineEnding::CrLf,
    )
    .await;

    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["hex"], "ff41");
    assert!(json["text"].is_null());
    assert!(json["text_lossy"].as_str().unwrap().contains('A'));
}

#[tokio::test]
async fn control_read_json_fail_on_timeout_returns_timeout_error() {
    let (input_tx, _input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();

    let response = handle_control_line(
        "read --timeout 0 --until-hex 0d0a --fail-on-timeout --json",
        &input_tx,
        &mut output_rx,
        &mut raw_rx,
        &history,
        &state,
        LineEnding::CrLf,
    )
    .await;

    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["code"], "timeout");
    assert!(json["error"].as_str().unwrap().contains("until-hex"));
}

#[tokio::test]
async fn control_raw_read_marks_truncated_history_incomplete() {
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(5)));
    {
        let mut history = history.lock().await;
        history.push(Source::Serial, b"1234".to_vec());
        history.push(Source::Serial, b"5678".to_vec());
    }

    let response = collect_control_raw_read(
        &mut raw_rx,
        &history,
        &ControlRawReadParams {
            source: ControlSource::Serial,
            timeout: Duration::from_millis(0),
            since: Some(1),
            until_hex: None,
            max_bytes: None,
            fail_on_timeout: false,
            raw_hex: true,
            raw_text: false,
        },
    )
    .await;

    assert_eq!(response.bytes, 4);
    assert_eq!(response.hex.as_deref(), Some("35363738"));
    assert_eq!(response.dropped_before, 5);
    assert!(!response.complete);
}

#[tokio::test]
async fn control_raw_read_recovers_lagged_broadcast_from_history() {
    let (raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));

    let read = tokio::spawn({
        let history = Arc::clone(&history);
        async move {
            collect_control_raw_read(
                &mut raw_rx,
                &history,
                &ControlRawReadParams {
                    source: ControlSource::Serial,
                    timeout: Duration::from_millis(100),
                    since: None,
                    until_hex: Some(b"abc".to_vec()),
                    max_bytes: None,
                    fail_on_timeout: false,
                    raw_hex: true,
                    raw_text: false,
                },
            )
            .await
        }
    });

    tokio::time::sleep(Duration::from_millis(10)).await;
    {
        let mut history = history.lock().await;
        for data in [b"a".as_slice(), b"b".as_slice(), b"c".as_slice()] {
            let data = data.to_vec();
            let seq = history.push(Source::Serial, data.clone());
            raw_tx
                .send(ControlOutput {
                    seq,
                    source: Source::Serial,
                    data,
                })
                .unwrap();
        }
    }

    let response = read.await.unwrap();
    assert_eq!(response.bytes, 3);
    assert_eq!(response.hex.as_deref(), Some("616263"));
    assert_eq!(response.next_seq, 4);
    assert!(response.complete);
    assert!(response.matched_until_hex);
}

#[tokio::test]
async fn control_request_hex_json_reports_raw_written_bytes() {
    let (input_tx, mut input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();

    let request = tokio::spawn({
        let history = Arc::clone(&history);
        let state = Arc::clone(&state);
        async move {
            handle_control_line(
                "request --hex --target rtt --timeout 0 --raw-text --json 41 54",
                &input_tx,
                &mut output_rx,
                &mut raw_rx,
                &history,
                &state,
                LineEnding::CrLf,
            )
            .await
        }
    });

    match input_rx.recv().await.unwrap() {
        InputEvent::Control(ControlRequest::Write {
            target,
            bytes,
            timeout,
            reply,
        }) => {
            assert_eq!(target, ControlTarget::Rtt);
            assert_eq!(bytes, b"AT");
            assert_eq!(timeout, Duration::from_millis(CONTROL_WRITE_ACK_TIMEOUT_MS));
            reply.send("OK rtt write 2 bytes\n".to_string()).unwrap();
        }
        other => panic!("unexpected input event: {other:?}"),
    }

    let response = request.await.unwrap();
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "request");
    assert_eq!(json["target"], "rtt");
    assert_eq!(json["actual_targets"], serde_json::json!(["rtt"]));
    assert_eq!(json["written_bytes"], 2);
    assert_eq!(json["write_timeout_ms"], CONTROL_WRITE_ACK_TIMEOUT_MS);
    assert_eq!(json["read_timeout_ms"], 0);
    assert_eq!(json["response"]["bytes"], 0);
    assert_eq!(json["response"]["text"], "");
    assert_eq!(json["response"]["text_lossy"], "");
}

#[tokio::test]
async fn control_write_json_wraps_transport_response() {
    let (input_tx, mut input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();

    let request = tokio::spawn({
        let history = Arc::clone(&history);
        let state = Arc::clone(&state);
        async move {
            handle_control_line(
                "write --target serial --json --timeout 100 AT",
                &input_tx,
                &mut output_rx,
                &mut raw_rx,
                &history,
                &state,
                LineEnding::CrLf,
            )
            .await
        }
    });

    match input_rx.recv().await.unwrap() {
        InputEvent::Control(ControlRequest::Write {
            target,
            bytes,
            timeout,
            reply,
        }) => {
            assert_eq!(target, ControlTarget::Serial);
            assert_eq!(bytes, b"AT");
            assert_eq!(timeout, Duration::from_millis(100));
            reply.send("OK serial write 2 bytes\n".to_string()).unwrap();
        }
        other => panic!("unexpected input event: {other:?}"),
    }

    let response = request.await.unwrap();
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "write");
    assert_eq!(json["target"], "serial");
    assert_eq!(json["actual_targets"], serde_json::json!(["serial"]));
    assert_eq!(json["bytes"], 2);
    assert_eq!(json["timeout_ms"], 100);
    assert_eq!(json["message"], "serial write 2 bytes");
}

#[tokio::test]
async fn control_write_json_without_selected_transport_returns_not_running_error() {
    let (input_tx, mut input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();

    let request = tokio::spawn({
        let history = Arc::clone(&history);
        let state = Arc::clone(&state);
        async move {
            handle_control_line(
                "write --target serial --json --timeout 100 AT",
                &input_tx,
                &mut output_rx,
                &mut raw_rx,
                &history,
                &state,
                LineEnding::CrLf,
            )
            .await
        }
    });

    match input_rx.recv().await.unwrap() {
        InputEvent::Control(ControlRequest::Write {
            target,
            bytes,
            reply,
            ..
        }) => {
            assert_eq!(target, ControlTarget::Serial);
            assert_eq!(bytes, b"AT");
            handle_control_request(
                ControlRequest::Write {
                    target,
                    bytes,
                    timeout: Duration::from_millis(100),
                    reply,
                },
                Route::Serial,
                &None,
                &None,
            )
            .await;
        }
        other => panic!("unexpected input event: {other:?}"),
    }

    let response = request.await.unwrap();
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["code"], "not_running");
    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("serial transport is not running"));
}

#[tokio::test]
async fn control_write_hex_json_serial_without_transport_returns_not_running_error() {
    let (input_tx, mut input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();

    let request = tokio::spawn({
        let history = Arc::clone(&history);
        let state = Arc::clone(&state);
        async move {
            handle_control_line(
                "write --hex --target serial --json --timeout 100 41",
                &input_tx,
                &mut output_rx,
                &mut raw_rx,
                &history,
                &state,
                LineEnding::CrLf,
            )
            .await
        }
    });

    match input_rx.recv().await.unwrap() {
        InputEvent::Control(ControlRequest::Write {
            target,
            bytes,
            reply,
            ..
        }) => {
            assert_eq!(target, ControlTarget::Serial);
            assert_eq!(bytes, b"A");
            handle_control_request(
                ControlRequest::Write {
                    target,
                    bytes,
                    timeout: Duration::from_millis(100),
                    reply,
                },
                Route::Serial,
                &None,
                &None,
            )
            .await;
        }
        other => panic!("unexpected input event: {other:?}"),
    }

    let response = request.await.unwrap();
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["code"], "not_running");
    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("serial transport is not running"));
}

#[tokio::test]
async fn control_write_hex_json_parse_error_returns_json_error() {
    let (input_tx, _input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();

    let response = handle_control_line(
        "write --hex --json zz",
        &input_tx,
        &mut output_rx,
        &mut raw_rx,
        &history,
        &state,
        LineEnding::CrLf,
    )
    .await;
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["code"], "invalid_argument");
    assert!(json["error"].as_str().unwrap().contains("invalid hex byte"));
}

#[tokio::test]
async fn control_write_json_times_out_waiting_for_reply() {
    let (input_tx, mut input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();

    let request = tokio::spawn({
        let history = Arc::clone(&history);
        let state = Arc::clone(&state);
        async move {
            handle_control_line(
                "write --target serial --json --timeout 1 AT",
                &input_tx,
                &mut output_rx,
                &mut raw_rx,
                &history,
                &state,
                LineEnding::CrLf,
            )
            .await
        }
    });

    match input_rx.recv().await.unwrap() {
        InputEvent::Control(ControlRequest::Write { bytes, reply, .. }) => {
            assert_eq!(bytes, b"AT");
            std::mem::forget(reply);
        }
        other => panic!("unexpected input event: {other:?}"),
    }

    let response = request.await.unwrap();
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["code"], "timeout");
    assert!(json["error"].as_str().unwrap().contains("timed out"));
}

#[tokio::test]
async fn control_read_json_parse_error_returns_json_error() {
    let (input_tx, _input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();

    let response = handle_control_line(
        "read --json --source nope",
        &input_tx,
        &mut output_rx,
        &mut raw_rx,
        &history,
        &state,
        LineEnding::CrLf,
    )
    .await;
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["code"], "unknown_option");
    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("unknown read option"));
}

#[tokio::test]
async fn control_request_hex_json_parse_error_returns_json_error() {
    let (input_tx, _input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();

    let response = handle_control_line(
        "request --hex --json zz",
        &input_tx,
        &mut output_rx,
        &mut raw_rx,
        &history,
        &state,
        LineEnding::CrLf,
    )
    .await;
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["code"], "invalid_argument");
    assert!(json["error"].as_str().unwrap().contains("invalid hex byte"));
}

#[tokio::test]
async fn control_reset_json_wraps_action_response() {
    let (input_tx, mut input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state_with_route(Route::Rtt);

    let request = tokio::spawn({
        let history = Arc::clone(&history);
        let state = Arc::clone(&state);
        async move {
            handle_control_line(
                "reset --json",
                &input_tx,
                &mut output_rx,
                &mut raw_rx,
                &history,
                &state,
                LineEnding::CrLf,
            )
            .await
        }
    });

    match input_rx.recv().await.unwrap() {
        InputEvent::Control(ControlRequest::Reset { reply }) => {
            reply.send("OK reset\n".to_string()).unwrap();
        }
        other => panic!("unexpected input event: {other:?}"),
    }

    let response = request.await.unwrap();
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "reset");
    assert_eq!(json["timeout_ms"], CONTROL_ACTION_TIMEOUT_MS);
    assert!(json["reported_result"].is_null());
    assert_eq!(json["message"], "reset");
}

#[tokio::test]
async fn control_reconnect_json_without_transports_returns_not_running_error() {
    let (input_tx, mut input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();

    let request = tokio::spawn({
        let history = Arc::clone(&history);
        let state = Arc::clone(&state);
        async move {
            handle_control_line(
                "reconnect --json",
                &input_tx,
                &mut output_rx,
                &mut raw_rx,
                &history,
                &state,
                LineEnding::CrLf,
            )
            .await
        }
    });

    match input_rx.recv().await.unwrap() {
        InputEvent::Control(ControlRequest::Reconnect { reply }) => {
            handle_control_request(
                ControlRequest::Reconnect { reply },
                Route::Both,
                &None,
                &None,
            )
            .await;
        }
        other => panic!("unexpected input event: {other:?}"),
    }

    let response = request.await.unwrap();
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["code"], "not_running");
    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("transport is not running"));
}

#[tokio::test]
async fn control_quit_json_wraps_action_response() {
    let (input_tx, mut input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();

    let response = handle_control_line(
        "quit --json",
        &input_tx,
        &mut output_rx,
        &mut raw_rx,
        &history,
        &state,
        LineEnding::CrLf,
    )
    .await;

    assert!(matches!(input_rx.recv().await.unwrap(), InputEvent::Quit));
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "quit");
    assert_eq!(json["message"], "quit");
}

#[tokio::test]
async fn control_quit_json_rejects_unknown_option_without_quitting() {
    let (input_tx, mut input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();

    let response = handle_control_line(
        "quit --json --timeout 1",
        &input_tx,
        &mut output_rx,
        &mut raw_rx,
        &history,
        &state,
        LineEnding::CrLf,
    )
    .await;

    assert!(input_rx.try_recv().is_err());
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["code"], "unknown_option");
    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("unknown quit option"));
}

#[tokio::test]
async fn control_reset_json_option_error_returns_json_error() {
    let (input_tx, _input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();

    let response = handle_control_line(
        "reset --json --bad",
        &input_tx,
        &mut output_rx,
        &mut raw_rx,
        &history,
        &state,
        LineEnding::CrLf,
    )
    .await;
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["code"], "unknown_option");
    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("unknown action option"));
}

#[tokio::test]
async fn control_reset_json_times_out_waiting_for_reply() {
    let (input_tx, mut input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state_with_route(Route::Rtt);

    let request = tokio::spawn({
        let history = Arc::clone(&history);
        let state = Arc::clone(&state);
        async move {
            handle_control_line(
                "reset --json --timeout 1",
                &input_tx,
                &mut output_rx,
                &mut raw_rx,
                &history,
                &state,
                LineEnding::CrLf,
            )
            .await
        }
    });

    match input_rx.recv().await.unwrap() {
        InputEvent::Control(ControlRequest::Reset { reply }) => {
            std::mem::forget(reply);
        }
        other => panic!("unexpected input event: {other:?}"),
    }

    let response = request.await.unwrap();
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["code"], "timeout");
    assert!(json["error"].as_str().unwrap().contains("timed out"));
}

#[tokio::test]
async fn control_reset_json_without_rtt_returns_unavailable_error() {
    let (input_tx, mut input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();

    let request = tokio::spawn({
        let history = Arc::clone(&history);
        let state = Arc::clone(&state);
        async move {
            handle_control_line(
                "reset --json",
                &input_tx,
                &mut output_rx,
                &mut raw_rx,
                &history,
                &state,
                LineEnding::CrLf,
            )
            .await
        }
    });
    match input_rx.recv().await.unwrap() {
        InputEvent::Control(ControlRequest::Reset { reply }) => {
            reply
                .send("ERR reset requires target flasher\n".to_string())
                .unwrap();
        }
        other => panic!("unexpected input event: {other:?}"),
    }
    let response = request.await.unwrap();
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["code"], "unavailable");
    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("requires target flasher"));
}

#[tokio::test]
async fn control_erase_json_without_rtt_returns_unavailable_error() {
    let (input_tx, mut input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();

    let request = tokio::spawn({
        let history = Arc::clone(&history);
        let state = Arc::clone(&state);
        async move {
            handle_control_line(
                "erase --json",
                &input_tx,
                &mut output_rx,
                &mut raw_rx,
                &history,
                &state,
                LineEnding::CrLf,
            )
            .await
        }
    });
    match input_rx.recv().await.unwrap() {
        InputEvent::Control(ControlRequest::Erase { reply }) => {
            reply
                .send("ERR erase requires target flasher\n".to_string())
                .unwrap();
        }
        other => panic!("unexpected input event: {other:?}"),
    }
    let response = request.await.unwrap();
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["code"], "unavailable");
    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("requires target flasher"));
}

#[tokio::test]
async fn control_erase_json_wraps_jlink_reported_result() {
    let (input_tx, mut input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state_with_route(Route::Rtt);

    let request = tokio::spawn({
        let history = Arc::clone(&history);
        let state = Arc::clone(&state);
        async move {
            handle_control_line(
                "erase --json",
                &input_tx,
                &mut output_rx,
                &mut raw_rx,
                &history,
                &state,
                LineEnding::CrLf,
            )
            .await
        }
    });

    match input_rx.recv().await.unwrap() {
        InputEvent::Control(ControlRequest::Erase { reply }) => {
            reply
                .send("OK erase done, J-Link reported chip erased\n".to_string())
                .unwrap();
        }
        other => panic!("unexpected input event: {other:?}"),
    }

    let response = request.await.unwrap();
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "erase");
    assert_eq!(json["timeout_ms"], CONTROL_ACTION_TIMEOUT_MS);
    assert_eq!(json["reported_result"], "chip erased");
    assert_eq!(json["message"], "erase done, J-Link reported chip erased");
}

#[tokio::test]
async fn control_reset_routes_to_serial_transport_when_serial_active() {
    let (serial_tx, mut serial_rx) = mpsc::channel(1);
    let (reply, _rx) = tokio::sync::oneshot::channel();

    handle_control_request(
        ControlRequest::Reset { reply },
        Route::Serial,
        &Some(serial_tx),
        &None,
    )
    .await;

    match serial_rx.recv().await.unwrap() {
        InterfaceCommand::Reset { reply: Some(reply) } => {
            let _ = reply.send("OK reset\n".to_string());
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[tokio::test]
async fn control_flash_routes_to_serial_transport_when_serial_active() {
    let root = std::env::temp_dir().join(format!(
        "rttio-test-{}-{}",
        std::process::id(),
        unique_test_id()
    ));
    fs::create_dir_all(&root).unwrap();
    let file = root.join("app.bin");
    fs::write(&file, b"firmware").unwrap();
    let (serial_tx, mut serial_rx) = mpsc::channel(1);
    let (reply, _rx) = tokio::sync::oneshot::channel();

    handle_control_request(
        ControlRequest::Flash {
            path: file.clone(),
            addr: 0x1000,
            reply,
        },
        Route::Serial,
        &Some(serial_tx),
        &None,
    )
    .await;

    match serial_rx.recv().await.unwrap() {
        InterfaceCommand::Flash { path, addr, reply } => {
            assert_eq!(path, file);
            assert_eq!(addr, 0x1000);
            let _ = reply
                .unwrap()
                .send("OK ESP flash done, wrote 8 bytes\n".to_string());
        }
        other => panic!("unexpected command: {other:?}"),
    }

    fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn control_erase_routes_to_serial_transport_when_serial_active() {
    let (serial_tx, mut serial_rx) = mpsc::channel(1);
    let (reply, _rx) = tokio::sync::oneshot::channel();

    handle_control_request(
        ControlRequest::Erase { reply },
        Route::Serial,
        &Some(serial_tx),
        &None,
    )
    .await;

    match serial_rx.recv().await.unwrap() {
        InterfaceCommand::Erase { reply: Some(reply) } => {
            let _ = reply.send("OK ESP erase done\n".to_string());
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[tokio::test]
async fn control_flash_json_validation_error_returns_json_error() {
    let (input_tx, _input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();

    let response = handle_control_line(
        "flash --json missing-file.hex",
        &input_tx,
        &mut output_rx,
        &mut raw_rx,
        &history,
        &state,
        LineEnding::CrLf,
    )
    .await;
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["code"], "invalid_path");
    assert!(json["error"].as_str().unwrap().contains("does not exist"));
}

#[tokio::test]
async fn control_flash_json_unsupported_extension_returns_invalid_path() {
    let root = std::env::temp_dir().join(format!(
        "rttio-test-{}-{}",
        std::process::id(),
        unique_test_id()
    ));
    fs::create_dir_all(&root).unwrap();
    let file = root.join("app.txt");
    fs::write(&file, b"not firmware").unwrap();

    let (input_tx, _input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();
    let command = format!("flash --json \"{}\"", file.display());

    let response = handle_control_line(
        &command,
        &input_tx,
        &mut output_rx,
        &mut raw_rx,
        &history,
        &state,
        LineEnding::CrLf,
    )
    .await;
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["code"], "invalid_path");
    assert!(json["error"].as_str().unwrap().contains("not a supported"));

    fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn control_flash_json_unknown_option_returns_json_error() {
    let (input_tx, _input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();

    let response = handle_control_line(
        "flash --json --bad app.hex",
        &input_tx,
        &mut output_rx,
        &mut raw_rx,
        &history,
        &state,
        LineEnding::CrLf,
    )
    .await;
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["code"], "unknown_option");
    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("unknown flash option"));
}

#[tokio::test]
async fn control_flash_json_without_rtt_returns_unavailable_error() {
    let root = std::env::temp_dir().join(format!(
        "rttio-test-{}-{}",
        std::process::id(),
        unique_test_id()
    ));
    fs::create_dir_all(&root).unwrap();
    let file = root.join("app.hex");
    fs::write(&file, b":00000001FF\n").unwrap();

    let (input_tx, mut input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state();
    let command = format!("flash --json --timeout 100 \"{}\"", file.display());

    let request = tokio::spawn({
        let history = Arc::clone(&history);
        let state = Arc::clone(&state);
        let command = command.clone();
        async move {
            handle_control_line(
                &command,
                &input_tx,
                &mut output_rx,
                &mut raw_rx,
                &history,
                &state,
                LineEnding::CrLf,
            )
            .await
        }
    });
    match input_rx.recv().await.unwrap() {
        InputEvent::Control(ControlRequest::Flash { reply, .. }) => {
            reply
                .send("ERR flash requires target flasher\n".to_string())
                .unwrap();
        }
        other => panic!("unexpected input event: {other:?}"),
    }
    let response = request.await.unwrap();
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["code"], "unavailable");
    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("requires target flasher"));

    fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn control_flash_json_wraps_file_and_addr() {
    let root = std::env::temp_dir().join(format!(
        "rttio-test-{}-{}",
        std::process::id(),
        unique_test_id()
    ));
    fs::create_dir_all(&root).unwrap();
    let file = root.join("app.hex");
    fs::write(&file, b":00000001FF\n").unwrap();

    let (input_tx, mut input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state_with_route(Route::Rtt);
    let command = format!("flash --json --timeout 100 \"{}\" 0x1000", file.display());

    let request = tokio::spawn({
        let history = Arc::clone(&history);
        let state = Arc::clone(&state);
        async move {
            handle_control_line(
                &command,
                &input_tx,
                &mut output_rx,
                &mut raw_rx,
                &history,
                &state,
                LineEnding::CrLf,
            )
            .await
        }
    });

    match input_rx.recv().await.unwrap() {
        InputEvent::Control(ControlRequest::Flash { path, addr, reply }) => {
            assert_eq!(path, file);
            assert_eq!(addr, 0x1000);
            reply
                .send("OK flash done, J-Link reported 16 bytes\n".to_string())
                .unwrap();
        }
        other => panic!("unexpected input event: {other:?}"),
    }

    let response = request.await.unwrap();
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "flash");
    assert_eq!(json["file"], file.display().to_string());
    assert_eq!(json["addr"], 0x1000);
    assert_eq!(json["timeout_ms"], 100);
    assert_eq!(json["reported_bytes"], 16);
    assert_eq!(json["message"], "flash done, J-Link reported 16 bytes");

    fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn control_flash_json_wraps_jlink_failure_as_json_error() {
    let root = std::env::temp_dir().join(format!(
        "rttio-test-{}-{}",
        std::process::id(),
        unique_test_id()
    ));
    fs::create_dir_all(&root).unwrap();
    let file = root.join("app.hex");
    fs::write(&file, b":00000001FF\n").unwrap();

    let (input_tx, mut input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = test_control_state_with_route(Route::Rtt);
    let command = format!("flash --json --timeout 100 \"{}\"", file.display());

    let request = tokio::spawn({
        let history = Arc::clone(&history);
        let state = Arc::clone(&state);
        async move {
            handle_control_line(
                &command,
                &input_tx,
                &mut output_rx,
                &mut raw_rx,
                &history,
                &state,
                LineEnding::CrLf,
            )
            .await
        }
    });

    match input_rx.recv().await.unwrap() {
        InputEvent::Control(ControlRequest::Flash { reply, .. }) => {
            reply
                .send("ERR flash failed: J-Link reported verify error\n".to_string())
                .unwrap();
        }
        other => panic!("unexpected input event: {other:?}"),
    }

    let response = request.await.unwrap();
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["code"], "command_failed");
    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("flash failed: J-Link reported verify error"));

    fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn control_write_waits_for_transport_reply() {
    let (tx, mut rx) = mpsc::channel(1);
    let writer = tokio::spawn(async move {
        route_write_control(
            ControlTarget::Serial,
            Route::Serial,
            b"AT",
            Duration::from_millis(CONTROL_WRITE_ACK_TIMEOUT_MS),
            &Some(tx),
            &None,
        )
        .await
    });

    match rx.recv().await.unwrap() {
        InterfaceCommand::Write { data, reply } => {
            assert_eq!(data, b"AT");
            send_optional_control_reply(reply, "OK serial write 2 bytes\n");
        }
        other => panic!("unexpected interface command: {other:?}"),
    }

    assert_eq!(writer.await.unwrap(), "OK serial write 2 bytes\n");
}

#[tokio::test]
async fn control_write_times_out_without_transport_reply() {
    let (tx, mut rx) = mpsc::channel(1);
    let writer = tokio::spawn(async move {
        route_write_control(
            ControlTarget::Serial,
            Route::Serial,
            b"AT",
            Duration::from_millis(1),
            &Some(tx),
            &None,
        )
        .await
    });

    match rx.recv().await.unwrap() {
        InterfaceCommand::Write { data, reply } => {
            assert_eq!(data, b"AT");
            std::mem::forget(reply);
        }
        other => panic!("unexpected interface command: {other:?}"),
    }

    assert_eq!(
        writer.await.unwrap(),
        "ERR serial transport write timed out\n"
    );
}

#[tokio::test]
async fn control_version_json_reports_build_metadata() {
    let (input_tx, _input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    let state = Arc::new(Mutex::new(ControlRuntimeState {
        control_socket: PathBuf::from("version-test.sock"),
        serial_configured: false,
        rtt_configured: true,
        serial_path: None,
        baud: None,
        jlink_sn: Some(801013229),
        jlink_ip: None,
        device: Some("nRF9151_xxCA".to_string()),
        rtt_tcp_host: None,
        rtt_tcp_port: None,
        rtt_up: 0,
        rtt_down: 0,
        serial_running: false,
        rtt_running: true,
        route: Route::Rtt,
        output_mode: OutputMode::Normal,
        timestamp: false,
        local_echo: true,
        output_paused: false,
        line_ending: LineEnding::CrLf,
    }));

    let response = handle_control_line(
        "version --json",
        &input_tx,
        &mut output_rx,
        &mut raw_rx,
        &history,
        &state,
        LineEnding::CrLf,
    )
    .await;
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();

    assert_eq!(json["ok"], true);
    assert_eq!(json["protocol"], "rttio-control");
    assert_eq!(json["version"], CONTROL_PROTOCOL_VERSION);
    assert_eq!(json["rttio_version"], RTTIO_VERSION);
    assert_eq!(json["git_hash"], RTTIO_GIT_HASH);
}

#[tokio::test]
async fn control_status_json_reports_runtime_and_history() {
    let (input_tx, _input_rx) = mpsc::channel(1);
    let (_output_tx, mut output_rx) = broadcast::channel(1);
    let (_raw_tx, mut raw_rx) = broadcast::channel(1);
    let history = Arc::new(Mutex::new(ControlHistory::new(1024)));
    history.lock().await.push(Source::Serial, b"abc".to_vec());
    history.lock().await.push(Source::Rtt, b"de".to_vec());
    let state = Arc::new(Mutex::new(ControlRuntimeState {
        control_socket: PathBuf::from("status-test.sock"),
        serial_configured: true,
        rtt_configured: true,
        serial_path: Some(PathBuf::from("/dev/tty.usbmodem101")),
        baud: Some(115200),
        jlink_sn: Some(801013229),
        jlink_ip: Some("192.168.1.10:19020".to_string()),
        device: Some("nRF9151_xxCA".to_string()),
        rtt_tcp_host: None,
        rtt_tcp_port: None,
        rtt_up: 0,
        rtt_down: 1,
        serial_running: true,
        rtt_running: true,
        route: Route::Both,
        output_mode: OutputMode::Hex,
        timestamp: true,
        local_echo: false,
        output_paused: false,
        line_ending: LineEnding::CrLf,
    }));

    let response = handle_control_line(
        "status --json",
        &input_tx,
        &mut output_rx,
        &mut raw_rx,
        &history,
        &state,
        LineEnding::CrLf,
    )
    .await;
    let json: serde_json::Value = serde_json::from_str(response.trim()).unwrap();

    assert_eq!(json["ok"], true);
    assert_eq!(json["protocol"], "rttio-control");
    assert_eq!(json["version"], CONTROL_PROTOCOL_VERSION);
    assert_eq!(json["rttio_version"], RTTIO_VERSION);
    assert_eq!(json["git_hash"], RTTIO_GIT_HASH);
    assert_eq!(json["pid"], std::process::id());
    assert_eq!(
        json["cwd"],
        std::env::current_dir().unwrap().display().to_string()
    );
    assert_eq!(json["control_socket"], "status-test.sock");
    assert_eq!(json["cursor_unit"], CONTROL_CURSOR_UNIT);
    assert_eq!(json["serial_configured"], true);
    assert_eq!(json["rtt_configured"], true);
    assert_eq!(json["serial_path"], "/dev/tty.usbmodem101");
    assert_eq!(json["baud"], 115200);
    assert_eq!(json["jlink_sn"], 801013229);
    assert_eq!(json["jlink_ip"], "192.168.1.10:19020");
    assert_eq!(json["device"], "nRF9151_xxCA");
    assert_eq!(json["rtt_up"], 0);
    assert_eq!(json["rtt_down"], 1);
    assert_eq!(json["route"], "both");
    assert_eq!(json["output_mode"], "hex");
    assert_eq!(json["line_ending"], "crlf");
    assert_eq!(json["history_max_bytes"], CONTROL_HISTORY_MAX_BYTES);
    assert_eq!(json["next_seq"], 6);
    assert_eq!(json["serial_next_seq"], 6);
    assert_eq!(json["serial_dropped_before"], 1);
    assert_eq!(json["rtt_next_seq"], 6);
    assert_eq!(json["rtt_dropped_before"], 4);
}

#[tokio::test]
async fn control_reset_without_rtt_returns_error() {
    let (reply, response) = tokio::sync::oneshot::channel();
    handle_control_request(ControlRequest::Reset { reply }, Route::Rtt, &None, &None).await;

    assert_eq!(
        response.await.unwrap(),
        "ERR reset requires target flasher\n".to_string()
    );
}

#[tokio::test]
async fn control_erase_without_rtt_returns_error() {
    let (reply, response) = tokio::sync::oneshot::channel();
    handle_control_request(ControlRequest::Erase { reply }, Route::Rtt, &None, &None).await;

    assert_eq!(
        response.await.unwrap(),
        "ERR erase requires target flasher\n".to_string()
    );
}

#[test]
fn config_loader_accepts_missing_version() {
    let path = temp_config_path("legacy");
    fs::write(
        &path,
        r#"{"baud":9600,"serial":"/dev/tty.test","unknown":"ignored"}"#,
    )
    .unwrap();

    let config = load_config_from_path(&path).unwrap();

    assert_eq!(config.version, CONFIG_VERSION);
    assert_eq!(config.baud, Some(9600));
    assert_eq!(config.serial, Some(PathBuf::from("/dev/tty.test")));
    let _ = fs::remove_file(path);
}

#[test]
fn config_loader_backs_up_invalid_json_and_defaults() {
    let path = temp_config_path("invalid");
    fs::write(&path, "{not json").unwrap();

    let config = load_config_from_path(&path).unwrap();

    assert_eq!(config.version, CONFIG_VERSION);
    assert!(config.serial.is_none());
    assert!(path
        .with_file_name(format!("{}.invalid", path_file_name(&path)))
        .exists());
    let _ = fs::remove_file(path.with_file_name(format!("{}.invalid", path_file_name(&path))));
    let _ = fs::remove_file(path);
}

#[test]
fn config_save_writes_current_version_and_repairs_recent_flash() {
    let path = temp_config_path("save");
    let mut config = RttioConfig {
        version: 99,
        ..RttioConfig::default()
    };
    config.recent_flash = (0..12)
        .map(|index| PathBuf::from(format!("firmware-{index}.hex")))
        .collect();

    save_config_to_path(&path, &config).unwrap();
    let saved = load_config_from_path(&path).unwrap();

    assert_eq!(saved.version, CONFIG_VERSION);
    assert_eq!(saved.recent_flash.len(), 10);
    let _ = fs::remove_file(path);
}

#[test]
fn config_tracks_recent_bin_flash_addresses_by_path() {
    let keep = PathBuf::from("build/app.bin");
    let drop = PathBuf::from("build/old.bin");
    let mut config = RttioConfig {
        recent_flash: vec![keep.clone()],
        recent_flash_addr: vec![
            RecentFlashAddress {
                path: keep.clone(),
                addr: 0x1000,
            },
            RecentFlashAddress {
                path: drop,
                addr: 0x2000,
            },
        ],
        ..RttioConfig::default()
    };

    config.normalize();

    assert_eq!(recent_flash_addr(&config, &keep), Some(0x1000));
    assert_eq!(config.recent_flash_addr.len(), 1);
}

fn temp_config_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "rttio-test-config-{name}-{}-{}.json",
        std::process::id(),
        unique_test_id()
    ))
}

fn path_file_name(path: &Path) -> &str {
    path.file_name().and_then(|name| name.to_str()).unwrap()
}

fn unique_test_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

fn bind_hardened_test_control_socket() -> (PathBuf, UnixListener) {
    let socket = std::env::temp_dir().join(format!(
        "rttio-test-{}-{}.sock",
        std::process::id(),
        unique_test_id()
    ));
    let listener = UnixListener::bind(&socket).unwrap();
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o600)).unwrap();
    (socket, listener)
}

fn test_control_state() -> Arc<Mutex<ControlRuntimeState>> {
    Arc::new(Mutex::new(ControlRuntimeState {
        control_socket: PathBuf::from(DEFAULT_CONTROL_SOCKET),
        serial_configured: false,
        rtt_configured: false,
        serial_path: None,
        baud: None,
        jlink_sn: None,
        jlink_ip: None,
        device: None,
        rtt_tcp_host: None,
        rtt_tcp_port: None,
        rtt_up: 0,
        rtt_down: 0,
        serial_running: false,
        rtt_running: false,
        route: Route::Both,
        output_mode: OutputMode::Normal,
        timestamp: false,
        local_echo: false,
        output_paused: false,
        line_ending: LineEnding::CrLf,
    }))
}

fn test_control_client_context(
    input_tx: mpsc::Sender<InputEvent>,
    terminal_tx: mpsc::Sender<TerminalEvent>,
    output_rx: broadcast::Receiver<String>,
    raw_rx: broadcast::Receiver<ControlOutput>,
    history: Arc<Mutex<ControlHistory>>,
    state: Arc<Mutex<ControlRuntimeState>>,
) -> ControlClientContext {
    ControlClientContext {
        input_tx,
        terminal_tx,
        output_rx,
        raw_rx,
        history,
        state,
        line_ending: LineEnding::CrLf,
    }
}

fn test_control_state_with_route(route: Route) -> Arc<Mutex<ControlRuntimeState>> {
    let state = test_control_state();
    {
        let mut state = state.try_lock().unwrap();
        state.route = route;
        match route {
            Route::Serial => {
                state.serial_configured = true;
                state.serial_running = true;
            }
            Route::Rtt => {
                state.rtt_configured = true;
                state.rtt_running = true;
            }
            Route::Both => {
                state.serial_configured = true;
                state.rtt_configured = true;
                state.serial_running = true;
                state.rtt_running = true;
            }
        }
    }
    state
}

#[cfg(feature = "control")]
#[test]
fn output_cursor_position_stays_above_status_bar_after_resize() {
    assert_eq!(output_cursor_position(10, 23, 80, 24), (10, 22));
    assert_eq!(output_cursor_position(999, 999, 80, 24), (79, 22));
    assert_eq!(output_cursor_position(5, 5, 0, 1), (0, 0));
}

#[cfg(feature = "control")]
#[test]
fn output_cursor_resize_moves_to_bottom_of_output_area() {
    let compressed = output_cursor_position(20, 30, 80, 8);
    assert_eq!(compressed, (20, 6));
    assert_eq!(
        output_cursor_after_resize(compressed, Some(8), 80, 30),
        (20, 28)
    );
}

#[cfg(feature = "control")]
#[test]
fn output_cursor_resize_preserves_non_bottom_position() {
    assert_eq!(
        output_cursor_after_resize((20, 6), Some(30), 80, 40),
        (20, 6)
    );
}

#[cfg(feature = "control")]
#[test]
fn output_cursor_tracking_handles_cr_lf_wrap_and_ansi() {
    let mut cursor = (0, 0);
    update_output_cursor_position_with_size("abc", &mut cursor, 5, 4);
    assert_eq!(cursor, (3, 0));

    update_output_cursor_position_with_size("\rZ\n", &mut cursor, 5, 4);
    assert_eq!(cursor, (0, 1));

    update_output_cursor_position_with_size("12345", &mut cursor, 5, 4);
    assert_eq!(cursor, (0, 2));

    update_output_cursor_position_with_size("\x1b[31mred\x1b[0m", &mut cursor, 10, 6);
    assert_eq!(cursor, (3, 2));
}

#[test]
fn terminal_output_buffers_incomplete_ansi_suffixes() {
    let mut pending = String::new();

    assert_eq!(take_complete_terminal_output(&mut pending, "a\x1b"), "a");
    assert_eq!(pending, "\x1b");

    assert_eq!(take_complete_terminal_output(&mut pending, "[0"), "");
    assert_eq!(pending, "\x1b[0");

    assert_eq!(
        take_complete_terminal_output(&mut pending, "m<inf>"),
        "\x1b[0m<inf>"
    );
    assert!(pending.is_empty());

    assert_eq!(
        take_complete_terminal_output(&mut pending, "\x1b[31mred\x1b[0m"),
        "\x1b[31mred\x1b[0m"
    );
    assert!(pending.is_empty());
}

#[test]
fn terminal_ansi_style_tracks_current_sgr_for_statusbar_restore() {
    let mut style = TerminalAnsiStyle::default();

    style.update("\x1b[33m<wrn> modem_manager:");
    assert_eq!(style.restore_sequence(), "\x1b[33m");

    style.update(" reset GPIO unavailable");
    assert_eq!(style.restore_sequence(), "\x1b[33m");

    style.update("\x1b[0;31m<err>");
    assert_eq!(style.restore_sequence(), "\x1b[0;31m");

    style.update("\x1b[0m");
    assert_eq!(style.restore_sequence(), "");
}

#[cfg(feature = "control")]
#[test]
fn flash_progress_status_segment_shows_action_and_percent() {
    let rendered = format_flash_progress(&TerminalFlashProgress {
        action: "flash 0x00000000".to_string(),
        percent: 42,
    });

    assert!(rendered.contains("flash:[####------]  42%"));
    assert!(rendered.contains("flash 0x00000000"));
}
