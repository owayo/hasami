//! 配布辞書の取得のテスト。HTTP は 127.0.0.1 に立てた小さなサーバーを相手にする（外には出ない）

use super::*;

#[test]
fn invalid_http_settings_are_rejected_without_exposing_credentials() {
    for options in [
        HttpOptions {
            user_agent: Some("client\r\nX-Injected: yes"),
            ..HttpOptions::default()
        },
        HttpOptions {
            proxy: ProxySetting::Url("http://user:secret@bad host"),
            ..HttpOptions::default()
        },
        HttpOptions {
            proxy: ProxySetting::Url("socks5://user:secret@localhost"),
            ..HttpOptions::default()
        },
    ] {
        let err = match Client::new(options) {
            Ok(_) => panic!("不正な設定を受け入れた"),
            Err(err) => err,
        };
        assert!(matches!(err, DownloadError::HttpConfig(_)));
        assert!(!err.to_string().contains("secret"));
        assert!(!format!("{err:?}").contains("secret"));
    }
}

/// 中身 `bytes` の大きさと SHA-256 を持つ、テスト用の配布辞書
fn distributed(name: &str, bytes: &[u8]) -> DistributedDict {
    DistributedDict::new(name, bytes.len() as u64, &to_hex(&Sha256::digest(bytes)))
}

/// `dict` に、`compressed`（中身）の圧縮版を足す
fn with_compressed(mut dict: DistributedDict, compressed: &[u8]) -> DistributedDict {
    dict.compressed = Some(CompressedFile {
        file: format!("{}.zst", dict.file),
        size: compressed.len() as u64,
        sha256: to_hex(&Sha256::digest(compressed)),
    });
    dict
}

fn zstd(bytes: &[u8]) -> Vec<u8> {
    ruzstd::encoding::compress_to_vec(bytes, ruzstd::encoding::CompressionLevel::Fastest)
}

fn sample_catalog() -> Catalog {
    Catalog {
        hasami_version: "26.9.104".into(),
        format_version: FORMAT_VERSION,
        recommended: "ipadic".into(),
        dictionaries: vec![with_compressed(distributed("ipadic", b"abc"), b"zst")],
    }
}

#[test]
fn release_urls_follow_the_version() {
    assert_eq!(CURRENT_TAG, concat!("v", env!("CARGO_PKG_VERSION")));
    assert_eq!(
        release_url("v26.9.104"),
        "https://github.com/owayo/hasami/releases/download/v26.9.104"
    );
    assert_eq!(
        catalog_url("https://example.com/mirror/"),
        "https://example.com/mirror/dictionaries.json"
    );
}

#[test]
fn every_distributed_dictionary_has_a_summary() {
    assert_eq!(RECOMMENDED, "ipadic-neologd-sudachi");
    for name in DISTRIBUTED_DICTS {
        assert!(summary(name).is_some_and(|s| !s.is_empty()), "{name}");
    }
    assert_eq!(summary("unidic"), None);
}

#[test]
fn to_hex_is_lowercase() {
    assert_eq!(to_hex(&[0x00, 0x0f, 0xa0, 0xff]), "000fa0ff");
    // 空の入力の SHA-256（よく知られた値）
    assert_eq!(
        to_hex(&Sha256::digest(b"")),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn the_catalog_round_trips_through_json() {
    let catalog = sample_catalog();
    let json = catalog.to_json();
    assert!(json.ends_with("}\n"), "{json}");
    assert_eq!(Catalog::parse(&json).unwrap(), catalog);
    // 空の summary・sources と、無い圧縮版は書かない（読むときは省いてよい）
    assert!(!json.contains("summary"), "{json}");
    assert!(!json.contains("sources"), "{json}");
    let plain = r#"{"hasami_version":"1.0.0","format_version":4,"recommended":"",
        "dictionaries":[{"name":"x","file":"x.hsd","size":1,
        "sha256":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"}]}"#;
    let parsed = Catalog::parse(plain).unwrap();
    assert_eq!(parsed.dictionaries[0].compressed, None);
    assert_eq!(parsed.find("x").unwrap().size, 1);
    assert!(parsed.find("y").is_none());
}

#[test]
fn the_catalog_rejects_names_outside_the_directory() {
    let mut bad = Vec::new();
    for file in [
        "../x.hsd", "a/b.hsd", "a\\b.hsd", ".x.hsd", "x.txt", "", "C:x.hsd",
    ] {
        let mut catalog = sample_catalog();
        catalog.dictionaries[0].file = file.into();
        bad.push((file, catalog));
    }
    for file in ["../x.hsd.zst", "x.hsd.gz", ".x.zst"] {
        let mut catalog = sample_catalog();
        catalog.dictionaries[0].compressed.as_mut().unwrap().file = file.into();
        bad.push((file, catalog));
    }
    for name in ["..", "a/b", ""] {
        let mut catalog = sample_catalog();
        catalog.dictionaries[0].name = name.into();
        catalog.recommended.clear();
        bad.push((name, catalog));
    }
    for (what, catalog) in bad {
        let err = Catalog::parse(&catalog.to_json()).unwrap_err();
        assert!(err.contains(&format!("{what:?}")), "{what}: {err}");
    }
}

#[test]
fn the_catalog_checks_hashes_duplicates_and_the_recommendation() {
    let mut upper = sample_catalog();
    upper.dictionaries[0].sha256 = upper.dictionaries[0].sha256.to_uppercase();
    assert!(
        Catalog::parse(&upper.to_json())
            .unwrap_err()
            .contains("SHA-256")
    );
    let mut short = sample_catalog();
    short.dictionaries[0].compressed.as_mut().unwrap().sha256 = "abc".into();
    assert!(
        Catalog::parse(&short.to_json())
            .unwrap_err()
            .contains("compressed")
    );
    let mut twice = sample_catalog();
    twice.dictionaries.push(twice.dictionaries[0].clone());
    assert!(
        Catalog::parse(&twice.to_json())
            .unwrap_err()
            .contains("duplicate")
    );
    let mut missing = sample_catalog();
    missing.recommended = "ipadic-neologd".into();
    assert!(
        Catalog::parse(&missing.to_json())
            .unwrap_err()
            .contains("recommended")
    );
    assert!(Catalog::parse("{").is_err());
}

#[test]
fn the_format_version_must_match() {
    let catalog = sample_catalog();
    assert!(catalog.check_format().is_ok());
    let mut other = catalog;
    other.format_version = FORMAT_VERSION + 1;
    let err = other.check_format().unwrap_err().to_string();
    assert!(err.contains(&format!("v{}", FORMAT_VERSION + 1)), "{err}");
    assert!(err.contains("26.9.104"), "{err}");
}

#[test]
fn a_hand_made_entry_downloads_the_raw_file() {
    let dict = DistributedDict::new("ipadic", 10, &"0".repeat(64));
    assert_eq!(dict.file, "ipadic.hsd");
    assert!(dict.validate().is_ok());
    assert_eq!(dict.transfer_size(true), 10);
    let dict = with_compressed(dict, b"abc");
    assert_eq!(dict.transfer_size(true), 3);
    assert_eq!(dict.transfer_size(false), 10);
}

#[test]
fn files_are_verified_by_size_and_hash() {
    let dir = tempfile::tempdir().unwrap();
    let dict = distributed("t", b"abcdef");
    let path = dir.path().join("t.hsd");
    assert_eq!(verify(&path, &dict).unwrap(), Verification::Missing);
    fs::write(&path, b"abcdef").unwrap();
    assert_eq!(verify(&path, &dict).unwrap(), Verification::Verified);
    // 大きさが違う
    fs::write(&path, b"abcdefg").unwrap();
    assert_eq!(verify(&path, &dict).unwrap(), Verification::Differs);
    // 大きさは同じで中身が違う
    fs::write(&path, b"abcdeg").unwrap();
    assert_eq!(verify(&path, &dict).unwrap(), Verification::Differs);
    // ディレクトリは辞書ではない
    fs::create_dir(dir.path().join("d.hsd")).unwrap();
    assert_eq!(
        verify(&dir.path().join("d.hsd"), &dict).unwrap(),
        Verification::Differs
    );
}

#[test]
fn zstd_frames_are_decoded_in_sequence() {
    // 2 つのフレームを続けたもの・スキップ可能なフレームを挟んだものも展開する
    let mut data = zstd(b"hello, ");
    // スキップ可能なフレーム（マジック 0x184D2A50、長さ 3）
    data.extend_from_slice(&0x184D_2A50u32.to_le_bytes());
    data.extend_from_slice(&3u32.to_le_bytes());
    data.extend_from_slice(b"xyz");
    data.extend_from_slice(&zstd(b"world"));
    let mut sink = Sink::new(Vec::new(), 100);
    assert!(decode_zstd(data.as_slice(), &mut sink).is_ok());
    assert_eq!(sink.out, b"hello, world");
    // 展開すると上限を超えるものは止める
    let mut small = Sink::new(Vec::new(), 5);
    assert!(matches!(
        decode_zstd(data.as_slice(), &mut small),
        Err(Decode::Output(SinkError::Over))
    ));
    // zstd でないもの
    let mut sink = Sink::new(Vec::new(), 100);
    assert!(decode_zstd(&b"not zstd data"[..], &mut sink).is_err());
}

#[cfg(feature = "build")]
mod with_dictionaries {
    //! 本物の（小さな）辞書を使うテスト。辞書を作るので `build` feature が要る

    use super::*;
    use crate::DictEntry;
    use crate::dict::DictBuilder;
    use std::io::{BufRead, BufReader};
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread;

    /// テスト用の小さな辞書（.hsd）のバイト列。`words` で中身を変える
    fn dictionary_bytes(words: &[(&str, &str)]) -> Vec<u8> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.hsd");
        let mut builder = DictBuilder::new();
        for &(surface, pos) in words {
            builder.add_entry(DictEntry {
                surface: surface.into(),
                left_id: 1,
                right_id: 1,
                cost: 1000,
                pos: pos.into(),
                base_form: surface.into(),
                ..Default::default()
            });
        }
        builder
            .write_hsd(&path, &builder.write_options(), |_, _| {})
            .unwrap();
        fs::read(&path).unwrap()
    }

    fn sample_bytes() -> Vec<u8> {
        dictionary_bytes(&[("猫", "名詞,一般,*,*"), ("です", "助動詞,*,*,*")])
    }

    /// サーバーの応答
    #[derive(Clone)]
    enum Reply {
        /// 200 と Content-Length 付きで本体を返す
        Body(Vec<u8>),
        /// 200 で、Content-Length を付けずに本体を返してから接続を閉じる
        CloseDelimited(Vec<u8>),
        /// 200 で、Content-Length に本体と違う値を書く
        WrongLength(Vec<u8>, u64),
        /// 本体を返さずにステータスだけ返す
        Status(u16, &'static str),
    }

    struct Server {
        url: String,
        requests: Arc<AtomicUsize>,
        received: Arc<Mutex<Vec<String>>>,
    }

    impl Server {
        fn requests(&self) -> usize {
            self.requests.load(Ordering::SeqCst)
        }
    }

    /// `routes` のパスへの GET に応答を返すサーバーを立てる（ほかのパスには 404）
    fn serve(routes: Vec<(&'static str, Reply)>) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(AtomicUsize::new(0));
        let count = Arc::clone(&requests);
        let received = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&received);
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                count.fetch_add(1, Ordering::SeqCst);
                // 読み手が途中でやめたときの書き込みの失敗は気にしない
                let _ = respond(stream, &routes, &log);
            }
        });
        Server {
            url,
            requests,
            received,
        }
    }

    fn respond(
        mut stream: TcpStream,
        routes: &[(&'static str, Reply)],
        log: &Mutex<Vec<String>>,
    ) -> io::Result<()> {
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        let mut reader = BufReader::new(stream.try_clone()?);
        let mut request_line = String::new();
        reader.read_line(&mut request_line)?;
        let mut request = request_line.clone();
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line)? == 0 || line == "\r\n" {
                break;
            }
            request.push_str(&line);
        }
        log.lock().unwrap().push(request);
        // 明示プロキシのテストでは外部ホストへ接続せず、この接続上で応答する。
        if request_line.starts_with("CONNECT ") {
            stream.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")?;
            stream.flush()?;
            return respond(stream, routes, log);
        }
        let requested = request_line.split_whitespace().nth(1).unwrap_or_default();
        let reply = routes
            .iter()
            .find(|(path, _)| request_line.starts_with("GET ") && *path == requested)
            .map_or(Reply::Status(404, "Not Found"), |(_, reply)| reply.clone());
        let (head, body) = match reply {
            Reply::Body(body) => (format!("200 OK\r\nContent-Length: {}", body.len()), body),
            Reply::CloseDelimited(body) => ("200 OK".to_string(), body),
            Reply::WrongLength(body, length) => {
                (format!("200 OK\r\nContent-Length: {length}"), body)
            }
            Reply::Status(code, reason) => {
                (format!("{code} {reason}\r\nContent-Length: 0"), Vec::new())
            }
        };
        stream.write_all(format!("HTTP/1.1 {head}\r\nConnection: close\r\n\r\n").as_bytes())?;
        stream.write_all(&body)?;
        stream.flush()
    }

    /// プロキシの環境変数に左右されない HTTP の設定（ほかは本番と同じ）
    fn test_agent() -> ureq::Agent {
        test_client().agent
    }

    fn test_client() -> Client {
        Client::new(HttpOptions {
            proxy: ProxySetting::None,
            ..HttpOptions::default()
        })
        .unwrap()
    }

    fn fetch(
        dict: &DistributedDict,
        dir: &Path,
        server: &Server,
        force: bool,
    ) -> Result<Outcome, DownloadError> {
        let options = DownloadOptions {
            base_url: Some(&server.url),
            force,
            ..DownloadOptions::default()
        };
        download_with(&test_agent(), dict, dir, options)
    }

    /// ディレクトリに残った一時ファイル
    fn parts(dir: &Path) -> Vec<String> {
        fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|name| name.ends_with(PART_SUFFIX))
            .collect()
    }

    #[test]
    fn a_dictionary_is_downloaded_verified_and_placed() {
        let bytes = sample_bytes();
        let dict = distributed("test", &bytes);
        let server = serve(vec![("/dict/test.hsd", Reply::Body(bytes.clone()))]);
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("share").join("hasami");
        let mut calls = Vec::new();
        let mut progress = |r, t| calls.push((r, t));
        // 取得元の末尾の / は重ねない。置き場所のディレクトリは作る
        let base = format!("{}/dict/", server.url);
        let options = DownloadOptions {
            base_url: Some(&base),
            progress: Some(&mut progress),
            ..DownloadOptions::default()
        };
        let outcome = download_with(&test_agent(), &dict, &dir, options).unwrap();
        let path = dir.join("test.hsd");
        assert_eq!(outcome, Outcome::Downloaded(path.clone()));
        assert_eq!(outcome.path(), path);
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert!(parts(&dir).is_empty(), "{:?}", parts(&dir));
        // 読める人は File::create で作ったファイルと同じ（一時ファイルの 0600 のままにしない）
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let reference = dir.join("reference");
            File::create(&reference).unwrap();
            let read_bits = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o444;
            assert_eq!(read_bits(&path), read_bits(&reference));
        }
        assert_eq!(server.requests(), 1);
        assert_eq!(calls.first(), Some(&(0, dict.size)));
        assert_eq!(calls.last(), Some(&(dict.size, dict.size)));
        assert!(Dictionary::load(&path).is_ok());

        // 取得済みなら通信しない（進み具合も呼ばない）
        let calls_before = calls.len();
        let mut progress = |r, t| calls.push((r, t));
        let options = DownloadOptions {
            base_url: Some(&base),
            progress: Some(&mut progress),
            ..DownloadOptions::default()
        };
        let again = download_with(&test_agent(), &dict, &dir, options).unwrap();
        assert_eq!(again, Outcome::Present(path));
        assert_eq!(server.requests(), 1);
        assert_eq!(calls.len(), calls_before);
    }

    #[test]
    fn compressed_404_notifies_before_retry_and_resets_progress() {
        let bytes = sample_bytes();
        let compressed = zstd(&bytes);
        let dict = with_compressed(distributed("test", &bytes), &compressed);
        let server = serve(vec![("/test.hsd", Reply::Body(bytes.clone()))]);
        let dir = tempfile::tempdir().unwrap();
        let mut progress = Vec::new();
        let mut events = Vec::new();
        let result = test_client()
            .download_with_events(
                &dict,
                dir.path(),
                DownloadOptions {
                    base_url: Some(&server.url),
                    progress: Some(&mut |r, t| progress.push((r, t))),
                    ..DownloadOptions::default()
                },
                &mut |event| {
                    assert_eq!(server.requests(), 1, "raw 要求より前に通知する");
                    events.push(event);
                },
            )
            .unwrap();
        assert_eq!(fs::read(result.path()).unwrap(), bytes);
        assert_eq!(
            events,
            [DownloadEvent::UncompressedFallback {
                compressed_url: format!("{}/test.hsd.zst", server.url),
                uncompressed_url: format!("{}/test.hsd", server.url),
            }]
        );
        assert_eq!(
            progress[..2],
            [(0, compressed.len() as u64), (0, dict.size)]
        );
        assert_eq!(progress.last(), Some(&(dict.size, dict.size)));
        let requests = server.received.lock().unwrap();
        assert!(requests[0].starts_with("GET /test.hsd.zst "));
        assert!(requests[1].starts_with("GET /test.hsd "));
        drop(requests);
        assert!(parts(dir.path()).is_empty());
        let present = test_client()
            .download_with_events(&dict, dir.path(), DownloadOptions::default(), &mut |_| {
                panic!("通信しないときは通知しない")
            })
            .unwrap();
        assert!(matches!(present, Outcome::Present(_)));
        assert_eq!(server.requests(), 2);
    }

    #[test]
    fn fallback_failure_preserves_the_existing_file() {
        let bytes = sample_bytes();
        let dict = with_compressed(distributed("test", &bytes), &zstd(&bytes));
        let mut corrupt = bytes.clone();
        corrupt[0] ^= 1;
        for reply in [Reply::Status(404, "Not Found"), Reply::Body(corrupt)] {
            let server = serve(vec![("/test.hsd", reply)]);
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("test.hsd");
            fs::write(&path, b"original").unwrap();
            let mut notified = 0;
            let err = test_client()
                .download_with_events(
                    &dict,
                    dir.path(),
                    DownloadOptions {
                        base_url: Some(&server.url),
                        force: true,
                        ..DownloadOptions::default()
                    },
                    &mut |_| {
                        notified += 1;
                        assert_eq!(server.requests(), 1);
                    },
                )
                .unwrap_err();
            assert!(
                matches!(
                    err,
                    DownloadError::Status { status: 404, .. } | DownloadError::Checksum { .. }
                ),
                "{err}"
            );
            assert_eq!(notified, 1);
            assert_eq!(server.requests(), 2);
            assert_eq!(fs::read(path).unwrap(), b"original");
            assert!(parts(dir.path()).is_empty());
        }
    }

    #[test]
    fn other_compressed_statuses_do_not_fall_back() {
        let bytes = sample_bytes();
        let dict = with_compressed(distributed("test", &bytes), &zstd(&bytes));
        for status in [304, 403, 429, 500] {
            let server = serve(vec![
                ("/test.hsd.zst", Reply::Status(status, "Error")),
                ("/test.hsd", Reply::Body(bytes.clone())),
            ]);
            let dir = tempfile::tempdir().unwrap();
            let err = test_client()
                .download_with_events(
                    &dict,
                    dir.path(),
                    DownloadOptions {
                        base_url: Some(&server.url),
                        ..DownloadOptions::default()
                    },
                    &mut |_| panic!("404 以外では通知しない"),
                )
                .unwrap_err();
            assert!(
                matches!(err, DownloadError::Status { status: actual, .. } if status == actual)
            );
            assert_eq!(server.requests(), 1);
            assert!(fs::read_dir(dir.path()).unwrap().next().is_none());
        }
    }

    #[test]
    fn a_connection_failure_does_not_fall_back() {
        let bytes = sample_bytes();
        let dict = with_compressed(distributed("test", &bytes), &zstd(&bytes));
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        // HTTP 応答を返さず閉じる。切り替えがあれば通知コールバックで検出する。
        let server_thread = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            assert!(line.starts_with("GET /test.hsd.zst "));
        });
        let dir = tempfile::tempdir().unwrap();
        let err = test_client()
            .download_with_events(
                &dict,
                dir.path(),
                DownloadOptions {
                    base_url: Some(&base_url),
                    ..DownloadOptions::default()
                },
                &mut |_| panic!("接続の失敗では切り替えない"),
            )
            .unwrap_err();
        assert!(matches!(err, DownloadError::Request { .. }));
        server_thread.join().unwrap();
        assert!(parts(dir.path()).is_empty());
    }

    #[test]
    fn custom_proxy_and_user_agent_apply_to_catalog_and_both_downloads() {
        let bytes = sample_bytes();
        let dict = with_compressed(distributed("test", &bytes), &zstd(&bytes));
        let mut catalog = sample_catalog();
        catalog.recommended = "test".into();
        catalog.dictionaries = vec![dict.clone()];
        let proxy = serve(vec![
            (
                "/dictionaries.json",
                Reply::Body(catalog.to_json().into_bytes()),
            ),
            ("/test.hsd", Reply::Body(bytes.clone())),
        ]);
        let client = Client::new(HttpOptions {
            proxy: ProxySetting::Url(&proxy.url),
            user_agent: Some("downstream-test/1.0"),
        })
        .unwrap();
        // このホストは解決できない。全要求が指定プロキシを通ったことも確かめる。
        let base_url = "http://dictionary.invalid";
        assert_eq!(client.catalog_from(base_url).unwrap(), catalog);
        let dir = tempfile::tempdir().unwrap();
        let result = client
            .download(
                &dict,
                dir.path(),
                DownloadOptions {
                    base_url: Some(base_url),
                    ..DownloadOptions::default()
                },
            )
            .unwrap();
        assert_eq!(fs::read(result.path()).unwrap(), bytes);
        let requests = proxy.received.lock().unwrap();
        let gets: Vec<_> = requests.iter().filter(|r| r.starts_with("GET ")).collect();
        assert_eq!(gets.len(), 3);
        for request in requests.iter() {
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("user-agent: downstream-test/1.0\r\n"),
                "{request}"
            );
        }
        assert!(gets[0].starts_with("GET /dictionaries.json "));
        assert!(gets[1].starts_with("GET /test.hsd.zst "));
        assert!(gets[2].starts_with("GET /test.hsd "));
    }

    #[test]
    fn disabling_proxy_ignores_environment_without_mutating_it() {
        const CHILD: &str = "HASAMI_TEST_PROXY_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command.args(["--exact", "download::tests::with_dictionaries::disabling_proxy_ignores_environment_without_mutating_it"]);
            command.env(CHILD, "1");
            for key in [
                "HTTP_PROXY",
                "HTTPS_PROXY",
                "ALL_PROXY",
                "http_proxy",
                "https_proxy",
                "all_proxy",
            ] {
                command.env(key, "http://127.0.0.1:0");
            }
            command.env("NO_PROXY", "").env("no_proxy", "");
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let catalog = sample_catalog();
        let server = serve(vec![(
            "/dictionaries.json",
            Reply::Body(catalog.to_json().into_bytes()),
        )]);
        assert!(matches!(
            Client::default().catalog_from(&server.url),
            Err(DownloadError::Request { .. })
        ));
        assert_eq!(test_client().catalog_from(&server.url).unwrap(), catalog);
        assert_eq!(server.requests(), 1);
        let expected = format!("user-agent: hasami/{}\r\n", env!("CARGO_PKG_VERSION"));
        assert!(
            server.received.lock().unwrap()[0]
                .to_ascii_lowercase()
                .contains(&expected)
        );
    }

    #[test]
    fn a_compressed_dictionary_is_decompressed_and_checked_twice() {
        let bytes = sample_bytes();
        let compressed = zstd(&bytes);
        let dict = with_compressed(distributed("test", &bytes), &compressed);
        let server = serve(vec![("/test.hsd.zst", Reply::Body(compressed.clone()))]);
        let dir = tempfile::tempdir().unwrap();
        let mut calls = Vec::new();
        let mut progress = |r, t| calls.push((r, t));
        let options = DownloadOptions {
            base_url: Some(&server.url),
            progress: Some(&mut progress),
            ..DownloadOptions::default()
        };
        let outcome = download_with(&test_agent(), &dict, dir.path(), options).unwrap();
        assert_eq!(outcome, Outcome::Downloaded(dir.path().join("test.hsd")));
        assert_eq!(fs::read(dir.path().join("test.hsd")).unwrap(), bytes);
        // 進み具合は受け取る量（圧縮版の大きさ）で数える
        assert_eq!(calls.first(), Some(&(0, compressed.len() as u64)));
        assert_eq!(
            calls.last(),
            Some(&(compressed.len() as u64, compressed.len() as u64))
        );
        assert!(parts(dir.path()).is_empty());
    }

    #[test]
    fn the_raw_file_can_be_chosen_over_the_compressed_one() {
        let bytes = sample_bytes();
        let dict = with_compressed(distributed("test", &bytes), &zstd(&bytes));
        // 圧縮版は置いていない
        let server = serve(vec![("/test.hsd", Reply::Body(bytes.clone()))]);
        let dir = tempfile::tempdir().unwrap();
        let options = DownloadOptions {
            base_url: Some(&server.url),
            compressed: false,
            ..DownloadOptions::default()
        };
        download_with(&test_agent(), &dict, dir.path(), options).unwrap();
        assert_eq!(fs::read(dir.path().join("test.hsd")).unwrap(), bytes);
    }

    #[test]
    fn a_different_file_is_not_replaced_without_force() {
        let bytes = sample_bytes();
        let dict = distributed("test", &bytes);
        let server = serve(vec![("/test.hsd", Reply::Body(bytes.clone()))]);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.hsd");
        fs::write(&path, b"another version").unwrap();

        let err = fetch(&dict, dir.path(), &server, false).unwrap_err();
        assert!(matches!(err, DownloadError::Differs { .. }), "{err}");
        assert!(err.to_string().contains("another version"), "{err}");
        assert_eq!(fs::read(&path).unwrap(), b"another version");
        assert_eq!(server.requests(), 0);

        // force なら置き換える
        let outcome = fetch(&dict, dir.path(), &server, true).unwrap();
        assert_eq!(outcome, Outcome::Downloaded(path.clone()));
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert!(parts(dir.path()).is_empty());
    }

    /// 取得に失敗したら、置き場所には何も置かず、一時ファイルも残さない
    fn assert_fails_and_leaves_nothing(
        dict: &DistributedDict,
        path: &'static str,
        reply: Reply,
        expected: impl Fn(&DownloadError) -> bool,
    ) -> String {
        let server = serve(vec![(path, reply)]);
        let dir = tempfile::tempdir().unwrap();
        let err = fetch(dict, dir.path(), &server, false).unwrap_err();
        assert!(expected(&err), "{err:?}");
        assert!(!dir.path().join(&dict.file).exists());
        assert!(parts(dir.path()).is_empty(), "{:?}", parts(dir.path()));
        assert_eq!(server.requests(), 1, "失敗時に別のファイルを要求しない");
        err.to_string()
    }

    #[test]
    fn a_different_hash_is_rejected() {
        let bytes = sample_bytes();
        let dict = distributed("test", &bytes);
        let mut other = bytes.clone();
        *other.last_mut().unwrap() ^= 0xff;
        let message =
            assert_fails_and_leaves_nothing(&dict, "/test.hsd", Reply::Body(other.clone()), |e| {
                matches!(e, DownloadError::Checksum { .. })
            });
        assert!(message.contains(&dict.sha256), "{message}");
        assert!(
            message.contains(&to_hex(&Sha256::digest(&other))),
            "{message}"
        );
    }

    #[test]
    fn a_compressed_file_is_checked_before_and_after_decompression() {
        let bytes = sample_bytes();
        let compressed = zstd(&bytes);
        // 受け取った圧縮版が目録と違う
        let dict = with_compressed(distributed("test", &bytes), &compressed);
        let mut other = compressed.clone();
        other.extend_from_slice(&zstd(b"")); // 空のフレームを足す（展開した中身は同じ）
        let wrong_size = dict.transfer_size(true) + other.len() as u64 - compressed.len() as u64;
        let mut padded = dict.clone();
        padded.compressed.as_mut().unwrap().size = wrong_size;
        assert_fails_and_leaves_nothing(
            &padded,
            "/test.hsd.zst",
            Reply::Body(other),
            |e| matches!(e, DownloadError::Checksum { from, .. } if !from.contains("decompressed")),
        );
        // 圧縮版は目録どおりだが、展開した中身が目録と違う
        let other_bytes = dictionary_bytes(&[("犬", "名詞,一般,*,*")]);
        let other_compressed = zstd(&other_bytes);
        let mut mismatched = with_compressed(distributed("test", &bytes), &other_compressed);
        mismatched.size = other_bytes.len() as u64;
        let message = assert_fails_and_leaves_nothing(
            &mismatched,
            "/test.hsd.zst",
            Reply::Body(other_compressed),
            |e| matches!(e, DownloadError::Checksum { from, .. } if from.contains("decompressed")),
        );
        assert!(message.contains(&mismatched.sha256), "{message}");
        // 展開すると目録の大きさを超える
        let mut small = with_compressed(distributed("test", &bytes), &compressed);
        small.size = bytes.len() as u64 - 1;
        assert_fails_and_leaves_nothing(
            &small,
            "/test.hsd.zst",
            Reply::Body(compressed),
            |e| matches!(e, DownloadError::Oversized { from, .. } if from.contains("decompressed")),
        );
        // zstd として壊れている（大きさと SHA-256 は、その中身で作った目録なので合う）
        let broken = b"this is not zstd".repeat(8);
        let dict = with_compressed(distributed("test", &bytes), &broken);
        assert_fails_and_leaves_nothing(&dict, "/test.hsd.zst", Reply::Body(broken), |e| {
            matches!(e, DownloadError::Decompress { .. })
        });
    }

    #[test]
    fn a_truncated_body_is_rejected() {
        let bytes = sample_bytes();
        let dict = distributed("test", &bytes);
        let half = bytes[..bytes.len() / 2].to_vec();
        // Content-Length がなく、接続が閉じて終わる
        let message = assert_fails_and_leaves_nothing(
            &dict,
            "/test.hsd",
            Reply::CloseDelimited(half.clone()),
            |e| matches!(e, DownloadError::Truncated { .. }),
        );
        assert!(message.contains("ended early"), "{message}");
        // Content-Length に届かないまま接続が閉じる
        assert_fails_and_leaves_nothing(
            &dict,
            "/test.hsd",
            Reply::WrongLength(half, dict.size),
            |e| matches!(e, DownloadError::Receive { .. }),
        );
        // 圧縮版が途中で切れる（展開のエラーではなく、途中で切れたと報告する）
        let compressed = zstd(&bytes);
        let dict = with_compressed(distributed("test", &bytes), &compressed);
        let cut = compressed[..compressed.len() / 2].to_vec();
        assert_fails_and_leaves_nothing(
            &dict,
            "/test.hsd.zst",
            Reply::CloseDelimited(cut),
            |e| matches!(e, DownloadError::Truncated { from, .. } if !from.contains("decompressed")),
        );
    }

    #[test]
    fn an_oversized_body_is_rejected() {
        let bytes = sample_bytes();
        let dict = distributed("test", &bytes);
        let mut longer = bytes.clone();
        longer.extend_from_slice(b"extra");
        assert_fails_and_leaves_nothing(&dict, "/test.hsd", Reply::CloseDelimited(longer), |e| {
            matches!(e, DownloadError::Oversized { .. })
        });
    }

    #[test]
    fn a_different_content_length_is_rejected_before_the_body() {
        let bytes = sample_bytes();
        let dict = distributed("test", &bytes);
        let message = assert_fails_and_leaves_nothing(
            &dict,
            "/test.hsd",
            Reply::WrongLength(bytes.clone(), dict.size + 10),
            |e| matches!(e, DownloadError::ContentLength { actual, .. } if *actual == dict.size + 10),
        );
        assert!(message.contains("Content-Length"), "{message}");
        // Content-Length: 0（ureq は本体なしとみなす）も、大きさの違いとして報告する
        assert_fails_and_leaves_nothing(&dict, "/test.hsd", Reply::Status(200, "OK"), |e| {
            matches!(e, DownloadError::ContentLength { actual: 0, .. })
        });
    }

    #[test]
    fn http_errors_are_reported() {
        let bytes = sample_bytes();
        let dict = distributed("test", &bytes);
        let message = assert_fails_and_leaves_nothing(
            &dict,
            "/test.hsd",
            Reply::Status(404, "Not Found"),
            |e| matches!(e, DownloadError::Status { status: 404, .. }),
        );
        assert!(message.contains("HTTP 404"), "{message}");
        assert!(message.contains("/test.hsd"), "{message}");
        // ureq がたどらない 3xx も失敗にする（既定では成功として返ってくる）
        assert_fails_and_leaves_nothing(
            &dict,
            "/test.hsd",
            Reply::Status(304, "Not Modified"),
            |e| matches!(e, DownloadError::Status { status: 304, .. }),
        );
    }

    #[test]
    fn a_body_that_is_not_a_dictionary_is_rejected() {
        // 大きさと SHA-256 は合う（その中身で作った項目）が、hasami の辞書ではない
        let bytes = b"this is not a hasami dictionary\n".repeat(64);
        let dict = distributed("test", &bytes);
        let message =
            assert_fails_and_leaves_nothing(&dict, "/test.hsd", Reply::Body(bytes.clone()), |e| {
                matches!(e, DownloadError::NotDictionary { .. })
            });
        assert!(message.contains("not a dictionary"), "{message}");
    }

    #[test]
    fn an_entry_with_an_unsafe_file_name_is_not_fetched() {
        let bytes = sample_bytes();
        let mut dict = distributed("test", &bytes);
        dict.file = "../test.hsd".into();
        let server = serve(vec![]);
        let dir = tempfile::tempdir().unwrap();
        let err = fetch(&dict, dir.path(), &server, false).unwrap_err();
        assert!(matches!(err, DownloadError::InvalidDict(_)), "{err}");
        assert_eq!(server.requests(), 0);
    }

    #[test]
    fn a_failed_forced_download_keeps_the_existing_file() {
        let bytes = sample_bytes();
        let dict = distributed("test", &bytes);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.hsd");
        fs::write(&path, &bytes).unwrap();
        for reply in [
            Reply::Status(500, "Internal Server Error"),
            Reply::Body(vec![0; bytes.len()]),
            Reply::CloseDelimited(bytes[..10].to_vec()),
        ] {
            let server = serve(vec![("/test.hsd", reply)]);
            assert!(fetch(&dict, dir.path(), &server, true).is_err());
            assert_eq!(server.requests(), 1, "force なら取得済みでも取り直す");
            assert_eq!(fs::read(&path).unwrap(), bytes);
            assert!(parts(dir.path()).is_empty(), "{:?}", parts(dir.path()));
        }
    }

    #[test]
    fn stale_partial_files_are_removed() {
        let bytes = sample_bytes();
        let dict = distributed("test", &bytes);
        let server = serve(vec![("/test.hsd", Reply::Body(bytes))]);
        let dir = tempfile::tempdir().unwrap();
        let aged = |name: &str, age: Duration| {
            let path = dir.path().join(name);
            let file = File::create(&path).unwrap();
            file.set_modified(SystemTime::now() - age).unwrap();
            path
        };
        let day = Duration::from_secs(24 * 60 * 60);
        let old = aged(".test.hsd.abc123.part", day + Duration::from_secs(3600));
        let fresh = aged(".test.hsd.def456.part", Duration::from_secs(60));
        // ほかの辞書の一時ファイルと、名前の形が違うファイルには触れない
        let other = aged(".test2.hsd.abc123.part", day * 2);
        let unrelated = aged(".test.hsd.abc123.keep", day * 2);

        fetch(&dict, dir.path(), &server, false).unwrap();
        assert!(!old.exists());
        assert!(fresh.exists());
        assert!(other.exists());
        assert!(unrelated.exists());
    }

    #[test]
    fn the_catalog_is_fetched_and_checked() {
        let catalog = sample_catalog();
        let server = serve(vec![
            (
                "/ok/dictionaries.json",
                Reply::Body(catalog.to_json().into_bytes()),
            ),
            ("/bad/dictionaries.json", Reply::Body(b"{\"x\":1}".to_vec())),
            (
                "/big/dictionaries.json",
                Reply::Body(vec![b' '; MAX_CATALOG_BYTES as usize + 1]),
            ),
        ]);
        let agent = test_agent();
        let fetched = catalog_with(&agent, &format!("{}/ok/", server.url)).unwrap();
        assert_eq!(fetched, catalog);
        for (path, what) in [
            ("bad", "invalid dictionary catalog"),
            ("big", "larger than"),
        ] {
            let err = catalog_with(&agent, &format!("{}/{path}", server.url)).unwrap_err();
            assert!(matches!(err, DownloadError::Catalog { .. }), "{err}");
            assert!(err.to_string().contains(what), "{err}");
        }
        let err = catalog_with(&agent, &format!("{}/none", server.url)).unwrap_err();
        assert!(
            matches!(err, DownloadError::Status { status: 404, .. }),
            "{err}"
        );
    }

    #[test]
    fn a_catalog_of_another_version_is_rejected() {
        let catalog = sample_catalog();
        let url = "https://example.com/dictionaries.json";
        assert!(ensure_version(catalog.clone(), "v26.9.104", url).is_ok());
        let err = ensure_version(catalog, "v26.9.105", url).unwrap_err();
        assert!(matches!(err, DownloadError::CatalogVersion { .. }), "{err}");
        assert!(err.to_string().contains("26.9.104"), "{err}");
    }

    #[test]
    fn a_local_file_is_installed_with_or_without_an_entry() {
        let bytes = sample_bytes();
        let compressed = zstd(&bytes);
        let dict = with_compressed(distributed("test", &bytes), &compressed);
        let src = tempfile::tempdir().unwrap();
        let raw = src.path().join("test.hsd");
        let zst = src.path().join("test.hsd.zst");
        fs::write(&raw, &bytes).unwrap();
        fs::write(&zst, &compressed).unwrap();

        // 目録の項目で確かめて置く（圧縮版は展開して置く）
        for input in [&raw, &zst] {
            let dir = tempfile::tempdir().unwrap();
            let outcome = install(input, Some(&dict), dir.path(), false).unwrap();
            assert_eq!(outcome, Outcome::Downloaded(dir.path().join("test.hsd")));
            assert_eq!(fs::read(outcome.path()).unwrap(), bytes);
            // 取得済みなら置き直さない
            assert_eq!(
                install(input, Some(&dict), dir.path(), false).unwrap(),
                Outcome::Present(dir.path().join("test.hsd"))
            );
            assert!(parts(dir.path()).is_empty());
        }
        // 項目がなければ、辞書全体を検証して、ファイルの名前で置く
        let dir = tempfile::tempdir().unwrap();
        let outcome = install(&zst, None, dir.path(), false).unwrap();
        assert_eq!(outcome.path(), dir.path().join("test.hsd"));
        assert_eq!(fs::read(outcome.path()).unwrap(), bytes);
        // 同じ中身なら置き直してよい。違う中身は force がなければ置き換えない
        assert!(install(&raw, None, dir.path(), false).is_ok());
        let other = src.path().join("other").join("test.hsd");
        fs::create_dir(other.parent().unwrap()).unwrap();
        fs::write(&other, dictionary_bytes(&[("犬", "名詞,一般,*,*")])).unwrap();
        let err = install(&other, None, dir.path(), false).unwrap_err();
        assert!(matches!(err, DownloadError::Differs { .. }), "{err}");
        assert_eq!(fs::read(dir.path().join("test.hsd")).unwrap(), bytes);
        assert!(install(&other, None, dir.path(), true).is_ok());
        assert_ne!(fs::read(dir.path().join("test.hsd")).unwrap(), bytes);
        assert!(parts(dir.path()).is_empty());
    }

    #[test]
    fn a_wrong_local_file_is_not_installed() {
        let bytes = sample_bytes();
        let dict = distributed("test", &bytes);
        let src = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        // 項目と中身が違う
        let other = src.path().join("test.hsd");
        fs::write(&other, dictionary_bytes(&[("犬", "名詞,一般,*,*")])).unwrap();
        assert!(install(&other, Some(&dict), dir.path(), false).is_err());
        // 辞書でない（項目なし）
        let text = src.path().join("text.hsd");
        fs::write(&text, b"not a dictionary").unwrap();
        let err = install(&text, None, dir.path(), false).unwrap_err();
        assert!(matches!(err, DownloadError::NotDictionary { .. }), "{err}");
        // 名前が .hsd で終わらない
        let named = src.path().join("test.bin");
        fs::write(&named, &bytes).unwrap();
        let err = install(&named, None, dir.path(), false).unwrap_err();
        assert!(matches!(err, DownloadError::InvalidDict(_)), "{err}");
        assert!(fs::read_dir(dir.path()).unwrap().next().is_none());
    }

    #[test]
    fn a_catalog_is_made_from_a_directory_of_dictionaries() {
        let dir = tempfile::tempdir().unwrap();
        let mut contents = Vec::new();
        for (i, name) in DISTRIBUTED_DICTS.iter().enumerate() {
            let bytes = dictionary_bytes(&[
                ("猫", "名詞,一般,*,*"),
                (&"犬".repeat(i + 1), "名詞,一般,*,*"),
            ]);
            fs::write(dir.path().join(format!("{name}.hsd")), &bytes).unwrap();
            contents.push(bytes);
        }
        // 圧縮版は一部の辞書だけ
        let ipadic = DISTRIBUTED_DICTS.len() - 1;
        let compressed = zstd(&contents[ipadic]);
        fs::write(dir.path().join("ipadic.hsd.zst"), &compressed).unwrap();

        let catalog = Catalog::from_dir(dir.path()).unwrap();
        assert_eq!(catalog.hasami_version, env!("CARGO_PKG_VERSION"));
        assert_eq!(catalog.format_version, FORMAT_VERSION);
        assert_eq!(catalog.recommended, RECOMMENDED);
        let names: Vec<&str> = catalog
            .dictionaries
            .iter()
            .map(|d| d.name.as_str())
            .collect();
        assert_eq!(names, DISTRIBUTED_DICTS);
        for (dict, bytes) in catalog.dictionaries.iter().zip(&contents) {
            assert_eq!(dict.size, bytes.len() as u64);
            assert_eq!(dict.sha256, to_hex(&Sha256::digest(bytes)));
            assert_eq!(dict.summary, summary(&dict.name).unwrap());
        }
        let ipadic = catalog.find("ipadic").unwrap();
        assert_eq!(
            ipadic.compressed,
            Some(CompressedFile {
                file: "ipadic.hsd.zst".into(),
                size: compressed.len() as u64,
                sha256: to_hex(&Sha256::digest(&compressed)),
            })
        );
        assert!(catalog.find(RECOMMENDED).unwrap().compressed.is_none());
        assert_eq!(Catalog::parse(&catalog.to_json()).unwrap(), catalog);

        // 展開すると元の辞書と違う圧縮版は拒む（大きさが違えば大きすぎる・途中で切れたとして）
        fs::write(dir.path().join("ipadic.hsd.zst"), zstd(&contents[0])).unwrap();
        let err = Catalog::from_dir(dir.path()).unwrap_err();
        assert!(
            matches!(
                err,
                DownloadError::Checksum { .. }
                    | DownloadError::Oversized { .. }
                    | DownloadError::Truncated { .. }
            ),
            "{err}"
        );
        assert!(
            err.to_string().contains("ipadic.hsd.zst (decompressed)"),
            "{err}"
        );
        // 配布辞書がそろっていなければ作らない
        fs::remove_file(dir.path().join("ipadic.hsd.zst")).unwrap();
        fs::remove_file(dir.path().join("ipadic-neologd.hsd")).unwrap();
        assert!(Catalog::from_dir(dir.path()).is_err());
    }
}

/// リリースの目録に届くことを確かめる（辞書の本体は受け取らない）。TLS の設定（provider と root_certs）の
/// 誤りは、実際に HTTPS でハンドシェイクするまで分からない
#[test]
#[ignore = "ネットワークが必要（この版のリリースに目録があること）"]
fn the_release_catalog_is_reachable() {
    let catalog = catalog(CURRENT_TAG).unwrap_or_else(|e| panic!("{CURRENT_TAG}: {e}"));
    assert!(catalog.check_format().is_ok());
    assert!(catalog.find(RECOMMENDED).is_some());
}
