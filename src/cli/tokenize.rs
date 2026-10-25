//! テキストと標準入力の解析、並列実行、結果の書式化を行う CLI。

use hasami::analyzer::{Analyzer, push_mecab, push_wakachi};
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::Instant;

/// 出力形式
#[derive(Clone, Copy, clap::ValueEnum)]
pub(crate) enum OutputFormat {
    /// MeCab互換形式
    Mecab,
    /// 分かち書き形式
    Wakachi,
    /// JSON形式
    Json,
}

/// 標準入力を 1 回の read で読むバイト数の上限（ファイルを流し込むと毎回この大きさのブロックになる）
const READ_CHUNK: usize = 1 << 18;
/// 標準出力のバッファの大きさ
const WRITE_BUFFER: usize = 1 << 16;
/// これより短いブロックは並列に解析しない（スレッドを立てる費用の方が大きい）
const PARALLEL_MIN_BYTES: usize = 1 << 15;
/// 並列に解析するとき、スレッドが 1 度に取る行のまとまりのバイト数の目安
const PIECE_BYTES: usize = 1 << 14;

pub(crate) fn run(
    dict_path: &Path,
    format: OutputFormat,
    text: Option<String>,
    threads: usize,
) -> io::Result<()> {
    let start = Instant::now();
    let analyzer = Analyzer::load(dict_path)?;
    eprintln!(
        "Dictionary loaded in {:.1}ms",
        start.elapsed().as_secs_f64() * 1000.0
    );

    let stdout = io::stdout();
    let mut out = io::BufWriter::with_capacity(WRITE_BUFFER, stdout.lock());

    if let Some(text) = text {
        let mut worker = Worker::new(analyzer);
        let mut buf = Vec::new();
        let tokens = worker.analyzer.try_tokenize(&text)?;
        write_output(&mut buf, &mut worker.line, &tokens, format);
        out.write_all(&buf)?;
        return out.flush();
    }
    let mut processor = BlockProcessor::new(analyzer, threads, format);
    process_input(&mut io::stdin().lock(), &mut out, &mut processor)
}

/// 入力を完結した行のブロックごとに解析する。
///
/// 読み込みは待つことがあるので、その前にそれまでの出力を書き出す
/// （行を送って結果を待つ相手とも詰まらない。大量の入力では読むたびに 1 回だけ書く）。
fn process_input(
    input: &mut impl Read,
    out: &mut impl Write,
    processor: &mut BlockProcessor,
) -> io::Result<()> {
    let mut pending: Vec<u8> = Vec::new();
    loop {
        out.flush()?;
        let len = pending.len();
        pending.resize(len + READ_CHUNK, 0);
        let n = loop {
            match input.read(&mut pending[len..]) {
                Ok(n) => break n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        };
        pending.truncate(len + n);
        let eof = n == 0;
        // 最後の改行までが完結した行（入力の終わりなら残りも 1 行）。前に読んだ残りには改行が無いので、
        // 今読んだ部分だけを探す（1 行がとても長い入力で、読み足すたびに全体を探し直さない）
        let end = if eof {
            pending.len()
        } else {
            match pending[len..].iter().rposition(|&b| b == b'\n') {
                Some(p) => len + p + 1,
                None => continue,
            }
        };
        match std::str::from_utf8(&pending[..end]) {
            Ok(block) => processor.process(block, out)?,
            Err(e) => {
                // 壊れた UTF-8 を含む行の手前までは解析して出してから、エラーにする
                let valid = &pending[..e.valid_up_to()];
                let cut = valid.iter().rposition(|&b| b == b'\n').map_or(0, |p| p + 1);
                let block = std::str::from_utf8(&valid[..cut]).map_err(io::Error::other)?;
                processor.process(block, out)?;
                out.flush()?;
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "stream did not contain valid UTF-8",
                ));
            }
        }
        pending.drain(..end);
        // とても長い行を読んだあとに大きな確保を持ち続けない
        if pending.capacity() > 4 * READ_CHUNK {
            pending.shrink_to(2 * READ_CHUNK);
        }
        if eof {
            break;
        }
    }
    out.flush()
}

/// 解析と書式化を受け持つワーカー
struct Worker {
    analyzer: Analyzer,
    /// 1 行分を書式化する作業用の文字列
    line: String,
}

impl Worker {
    fn new(analyzer: Analyzer) -> Self {
        Worker {
            analyzer,
            line: String::new(),
        }
    }

    /// 行を順に解析して `out` に書く。空行（空白だけの行を含む）は飛ばす
    ///
    /// エラーのときは、それまでの行の出力を `out` に残して返す
    fn process(
        &mut self,
        lines: &[&str],
        format: OutputFormat,
        out: &mut Vec<u8>,
    ) -> io::Result<()> {
        for line in lines {
            let text = line.trim();
            if text.is_empty() {
                continue;
            }
            let tokens = self.analyzer.try_tokenize(text)?;
            write_output(out, &mut self.line, &tokens, format);
        }
        Ok(())
    }
}

/// 行のまとまり 1 つ分の出力（ブロックをまたいで確保を使い回す）
#[derive(Default)]
struct Piece {
    out: Vec<u8>,
    error: Option<io::Error>,
}

/// 完結した行のブロックを解析して書式化する（ワーカーと出力の確保を使い回す）
struct BlockProcessor {
    workers: Vec<Worker>,
    pieces: Vec<Mutex<Piece>>,
    threads: usize,
    format: OutputFormat,
}

impl BlockProcessor {
    fn new(analyzer: Analyzer, threads: usize, format: OutputFormat) -> Self {
        BlockProcessor {
            workers: vec![Worker::new(analyzer)],
            pieces: Vec::new(),
            threads,
            format,
        }
    }

    /// ブロックを解析して `out` に書く
    ///
    /// 行を連続したまとまり（[`PIECE_BYTES`] ほど）に分ける。ブロックが大きく `threads` が 2 以上なら、
    /// スレッドが空いた順にまとまりを取って解析する（P コアと E コアのように速さの違うコアが混ざっても
    /// 遅いスレッドを待たない）。出力は入力の順に、まとまりごとに書く。エラーは入力の順で最初のものを
    /// 返し、その手前の行の出力だけを書く（1 行ずつ順に解析したときと同じ出力とエラーになる）。
    fn process(&mut self, block: &str, out: &mut impl Write) -> io::Result<()> {
        let lines: Vec<&str> = block.lines().collect();
        let ranges = split_pieces(&lines, PIECE_BYTES);
        if ranges.is_empty() {
            return Ok(());
        }
        if self.pieces.len() < ranges.len() {
            self.pieces.resize_with(ranges.len(), Default::default);
        }
        let format = self.format;
        if self.threads < 2 || block.len() < PARALLEL_MIN_BYTES {
            let piece = self.pieces[0]
                .get_mut()
                .unwrap_or_else(PoisonError::into_inner);
            let worker = &mut self.workers[0];
            for range in ranges {
                let result = worker.process(&lines[range], format, &mut piece.out);
                out.write_all(&piece.out)?;
                piece.out.clear();
                result?;
            }
            return Ok(());
        }

        let threads = self.threads.min(ranges.len());
        while self.workers.len() < threads {
            let analyzer = self.workers[0].analyzer.clone();
            self.workers.push(Worker::new(analyzer));
        }
        let next = AtomicUsize::new(0);
        let pieces = &self.pieces[..ranges.len()];
        let run = |worker: &mut Worker| {
            loop {
                let k = next.fetch_add(1, Ordering::Relaxed);
                let Some(range) = ranges.get(k) else {
                    break;
                };
                let mut piece = pieces[k].lock().unwrap_or_else(PoisonError::into_inner);
                let piece = &mut *piece;
                piece.error = worker
                    .process(&lines[range.clone()], format, &mut piece.out)
                    .err();
            }
        };
        std::thread::scope(|s| {
            let (first, rest) = self.workers[..threads]
                .split_first_mut()
                .expect("at least one worker");
            for worker in rest {
                s.spawn(move || run(worker));
            }
            run(first);
        });

        let mut result = Ok(());
        for piece in &mut self.pieces[..ranges.len()] {
            let piece = piece.get_mut().unwrap_or_else(PoisonError::into_inner);
            let error = piece.error.take();
            if result.is_ok() {
                out.write_all(&piece.out)?;
                if let Some(e) = error {
                    result = Err(e);
                }
            }
            piece.out.clear();
        }
        result
    }
}

/// 行を、先頭から順に `piece_bytes` バイトほどずつの連続した区間に分ける
fn split_pieces(lines: &[&str], piece_bytes: usize) -> Vec<std::ops::Range<usize>> {
    let mut pieces = Vec::new();
    let mut start = 0;
    let mut bytes = 0;
    for (i, line) in lines.iter().enumerate() {
        bytes += line.len() + 1;
        if bytes >= piece_bytes {
            pieces.push(start..i + 1);
            start = i + 1;
            bytes = 0;
        }
    }
    if start < lines.len() {
        pieces.push(start..lines.len());
    }
    pieces
}

/// 1 行分の解析結果を書式化して `out` に足す。`line` は作業用
fn write_output(
    out: &mut Vec<u8>,
    line: &mut String,
    tokens: &[hasami::Token],
    format: OutputFormat,
) {
    match format {
        OutputFormat::Wakachi => {
            line.clear();
            push_wakachi(line, tokens);
            line.push('\n');
            out.extend_from_slice(line.as_bytes());
        }
        OutputFormat::Json => write_json(out, tokens),
        OutputFormat::Mecab => {
            line.clear();
            push_mecab(line, tokens);
            out.extend_from_slice(line.as_bytes());
        }
    }
}

const WRITE_VEC: &str = "writing to a Vec<u8> does not fail";

/// トークンの配列を 1 行の JSON で書く（キーはアルファベット順。serde_json の Value と同じ並び）
fn write_json(out: &mut Vec<u8>, tokens: &[hasami::Token]) {
    // 文字列のエスケープは serde_json に任せる（Vec<u8> への書き込みは失敗しない）
    fn string(out: &mut Vec<u8>, s: &str) {
        serde_json::to_writer(&mut *out, s).expect(WRITE_VEC);
    }
    out.push(b'[');
    for (i, t) in tokens.iter().enumerate() {
        if i > 0 {
            out.push(b',');
        }
        out.extend_from_slice(b"{\"base_form\":");
        string(out, &t.base_form);
        out.extend_from_slice(b",\"conj_form\":");
        string(out, &t.conj_form);
        out.extend_from_slice(b",\"conj_type\":");
        string(out, &t.conj_type);
        write!(out, ",\"end\":{},\"is_known\":{}", t.end, t.is_known).expect(WRITE_VEC);
        out.extend_from_slice(b",\"pos\":");
        string(out, &t.pos);
        out.extend_from_slice(b",\"pronunciation\":");
        string(out, &t.pronunciation);
        out.extend_from_slice(b",\"reading\":");
        string(out, &t.reading);
        write!(out, ",\"start\":{},\"surface\":", t.start).expect(WRITE_VEC);
        string(out, &t.surface);
        out.push(b'}');
    }
    out.extend_from_slice(b"]\n");
}

#[cfg(test)]
mod tests {
    use super::*;
    use hasami::dict::DictBuilder;

    #[test]
    fn test_split_pieces_covers_all_lines_in_order() {
        let lines = ["a", "bbbb", "", "cc", "dddddddd", "e", "ff"];
        for piece_bytes in 1..=30 {
            let pieces = split_pieces(&lines, piece_bytes);
            assert_eq!(pieces[0].start, 0);
            assert_eq!(pieces.last().unwrap().end, lines.len());
            for pair in pieces.windows(2) {
                assert_eq!(pair[0].end, pair[1].start);
            }
            assert!(pieces.iter().all(|p| !p.is_empty()));
        }
        assert_eq!(split_pieces(&lines, 5), vec![0..2, 2..5, 5..7]);
        assert!(split_pieces(&[], 3).is_empty());
    }

    fn test_analyzer() -> Analyzer {
        use hasami::DictEntry;
        let mut builder = DictBuilder::new();
        for (surface, pos, reading) in [
            ("東京", "名詞,固有名詞,地域,一般", "トウキョウ"),
            ("都", "名詞,接尾,地域,*", "ト"),
            ("に", "助詞,格助詞,一般,*", "ニ"),
            ("住む", "動詞,自立,*,*", "スム"),
            ("\"引用\"", "名詞,一般,*,*", "インヨウ"),
        ] {
            builder.add_entry(DictEntry {
                surface: surface.into(),
                cost: 100,
                pos: pos.into(),
                base_form: surface.into(),
                reading: reading.into(),
                pronunciation: reading.into(),
                ..Default::default()
            });
        }
        Analyzer::from_dict(builder.build().unwrap())
    }

    #[test]
    fn test_json_output_matches_serde_json_values() {
        let mut analyzer = test_analyzer();
        for text in ["東京都に住む", "\"引用\"とA\\B\u{1}\t\u{7f}é", "X"] {
            let tokens = analyzer.tokenize(text);
            let values: Vec<serde_json::Value> = tokens
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "surface": &*t.surface,
                        "start": t.start,
                        "end": t.end,
                        "pos": &*t.pos,
                        "conj_type": &*t.conj_type,
                        "conj_form": &*t.conj_form,
                        "base_form": &*t.base_form,
                        "reading": &*t.reading,
                        "pronunciation": &*t.pronunciation,
                        "is_known": t.is_known,
                    })
                })
                .collect();
            let expected = format!("{}\n", serde_json::to_string(&values).unwrap());
            let mut out = Vec::new();
            write_json(&mut out, &tokens);
            assert_eq!(String::from_utf8(out).unwrap(), expected);
        }
    }

    #[test]
    fn test_parallel_blocks_match_sequential_output() {
        let base = test_analyzer();
        // 空行・空白だけの行・CRLF を含む
        let block: String = (0..2000)
            .map(|i| match i % 7 {
                0 => "\n".to_string(),
                1 => "  \t \r\n".to_string(),
                2 => format!("東京都に住む{i}\r\n"),
                _ => format!("{}X{i}\n", "東京に住む".repeat(i % 5 + 1)),
            })
            .collect();
        for format in [
            OutputFormat::Mecab,
            OutputFormat::Wakachi,
            OutputFormat::Json,
        ] {
            let mut expected = Vec::new();
            BlockProcessor::new(base.clone(), 1, format)
                .process(&block, &mut expected)
                .unwrap();
            assert!(!expected.is_empty());
            for threads in [2, 3, 8] {
                let mut processor = BlockProcessor::new(base.clone(), threads, format);
                // 同じ処理器でブロックを続けて処理しても（出力の確保を使い回しても）同じ出力
                for _ in 0..2 {
                    let mut out = Vec::new();
                    processor.process(&block, &mut out).unwrap();
                    assert!(
                        processor.workers.len() > 1,
                        "block is large enough to split"
                    );
                    assert_eq!(out, expected, "threads={threads}");
                }
            }
        }
    }

    /// 文字の途中でも読み込みを区切り、各読み込みの前に割り込みを返す。
    struct FragmentedInput<'a> {
        bytes: &'a [u8],
        chunk: usize,
        interrupt: bool,
    }

    impl Read for FragmentedInput<'_> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.interrupt = !self.interrupt;
            if self.interrupt {
                return Err(io::ErrorKind::Interrupted.into());
            }
            let len = buf.len().min(self.chunk);
            self.bytes.read(&mut buf[..len])
        }
    }

    #[test]
    fn test_stream_handles_fragmented_utf8_and_interrupted_reads() {
        let text = "東京\r\n\n \t \r\n  都に住む  \nX";
        let base = test_analyzer();
        for format in [
            OutputFormat::Mecab,
            OutputFormat::Wakachi,
            OutputFormat::Json,
        ] {
            let mut expected = Vec::new();
            BlockProcessor::new(base.clone(), 1, format)
                .process(text, &mut expected)
                .unwrap();
            for chunk in [1, 2, 7, READ_CHUNK] {
                let mut input = FragmentedInput {
                    bytes: text.as_bytes(),
                    chunk,
                    interrupt: false,
                };
                let mut processor = BlockProcessor::new(base.clone(), 4, format);
                let mut out = Vec::new();
                process_input(&mut input, &mut out, &mut processor).unwrap();
                assert_eq!(out, expected, "chunk={chunk}");
            }
        }
    }

    #[test]
    fn test_stream_keeps_only_complete_lines_before_invalid_utf8() {
        let base = test_analyzer();
        let mut expected = Vec::new();
        BlockProcessor::new(base.clone(), 1, OutputFormat::Wakachi)
            .process("東京\n", &mut expected)
            .unwrap();
        for suffix in [b"\xff\nX\n".as_slice(), b"\xe6\x9d"] {
            let mut bytes = "東京\n都".as_bytes().to_vec();
            bytes.extend_from_slice(suffix);
            for chunk in [1, READ_CHUNK] {
                let mut input = FragmentedInput {
                    bytes: &bytes,
                    chunk,
                    interrupt: false,
                };
                let mut processor = BlockProcessor::new(base.clone(), 4, OutputFormat::Wakachi);
                let mut out = Vec::new();
                let error = process_input(&mut input, &mut out, &mut processor).unwrap_err();
                assert_eq!(error.kind(), io::ErrorKind::InvalidData);
                assert_eq!(out, expected, "chunk={chunk}, suffix={suffix:?}");
            }
        }
    }

    #[test]
    fn test_stream_handles_lines_larger_than_read_chunk() {
        let text = format!("{}\n都", "東京".repeat(READ_CHUNK / "東京".len() + 1));
        let base = test_analyzer();
        let mut expected = Vec::new();
        BlockProcessor::new(base.clone(), 1, OutputFormat::Wakachi)
            .process(&text, &mut expected)
            .unwrap();
        let mut processor = BlockProcessor::new(base, 4, OutputFormat::Wakachi);
        let mut out = Vec::new();
        process_input(&mut text.as_bytes(), &mut out, &mut processor).unwrap();
        assert_eq!(out, expected);
    }

    struct FailingInput;

    impl Read for FailingInput {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::ErrorKind::ConnectionReset.into())
        }
    }

    struct FailingOutput {
        on_flush: bool,
    }

    impl Write for FailingOutput {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }

        fn flush(&mut self) -> io::Result<()> {
            if self.on_flush {
                Err(io::ErrorKind::BrokenPipe.into())
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn test_stream_propagates_read_errors_after_writing_complete_lines() {
        let mut processor = BlockProcessor::new(test_analyzer(), 1, OutputFormat::Wakachi);
        let mut input = "東京\n".as_bytes().chain(FailingInput);
        let mut out = Vec::new();
        let error = process_input(&mut input, &mut out, &mut processor).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::ConnectionReset);
        assert_eq!(out, "東京\n".as_bytes());
    }

    #[test]
    fn test_stream_propagates_write_and_flush_errors() {
        for on_flush in [false, true] {
            let mut processor = BlockProcessor::new(test_analyzer(), 1, OutputFormat::Wakachi);
            let error = process_input(
                &mut "東京\n".as_bytes(),
                &mut FailingOutput { on_flush },
                &mut processor,
            )
            .unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        }
    }
}
