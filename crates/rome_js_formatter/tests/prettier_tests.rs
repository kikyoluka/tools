use parking_lot::{const_mutex, Mutex};
use rome_rowan::{TextRange, TextSize};
use similar::{utils::diff_lines, Algorithm, ChangeTag};
use std::{
    env,
    ffi::OsStr,
    fmt::Write,
    fs::{read_to_string, write},
    ops::Range,
    os::raw::c_int,
    path::Path,
    sync::Once,
};

use rome_diagnostics::{file::SimpleFiles, termcolor, Emitter};
use rome_formatter::IndentStyle;
use rome_js_formatter::context::JsFormatContext;
use rome_js_parser::parse;
use rome_js_syntax::SourceType;

use crate::check_reformat::CheckReformatParams;

mod check_reformat;

tests_macros::gen_tests! {"tests/specs/prettier/{js,typescript}/**/*.{js,ts,jsx,tsx}", crate::test_snapshot, "script"}

const PRETTIER_IGNORE: &str = "prettier-ignore";
const ROME_IGNORE: &str = "rome-ignore format: prettier ignore";

fn test_snapshot(input: &'static str, _: &str, _: &str, _: &str) {
    countme::enable(true);

    if input.contains("flow") || input.contains("prepare_tests") {
        return;
    }

    let input_file = Path::new(input);
    let file_name = input_file.file_name().and_then(OsStr::to_str).unwrap();
    let mut input_code = read_to_string(input_file)
        .unwrap_or_else(|err| panic!("failed to read {:?}: {:?}", input_file, err));

    let (_, range_start_index, range_end_index) = strip_placeholders(&mut input_code);
    let parse_input = input_code.replace(PRETTIER_IGNORE, ROME_IGNORE);

    // Prettier testing suite uses JSX tags inside JS files.
    // As there's no way to know in advance which files have JSX syntax, we
    // change the source type only here
    let source_type = if input_file.extension().unwrap() == "js" {
        SourceType::jsx()
    } else if file_name.contains("jsx") && input_file.extension() == Some(OsStr::new("ts")) {
        SourceType::tsx()
    } else {
        input_file.try_into().unwrap()
    };

    let parsed = parse(&parse_input, 0, source_type);

    let has_errors = parsed.has_errors();
    let syntax = parsed.syntax();

    let context = JsFormatContext {
        indent_style: IndentStyle::Space(2),
        ..JsFormatContext::default()
    };

    let result = match (range_start_index, range_end_index) {
        (Some(start), Some(end)) => {
            // Skip the reversed range tests as its impossible
            // to create a reversed TextRange anyway
            if end < start {
                return;
            }

            rome_js_formatter::format_range(
                context,
                &syntax,
                TextRange::new(
                    TextSize::try_from(start).unwrap(),
                    TextSize::try_from(end).unwrap(),
                ),
            )
        }
        _ => rome_js_formatter::format_node(context, &syntax).map(|formatted| formatted.print()),
    };

    let formatted = result.expect("formatting failed");
    let formatted = match (range_start_index, range_end_index) {
        (Some(_), Some(_)) => {
            let range = formatted
                .range()
                .expect("the result of format_range should have a range");

            let formatted = formatted.as_code();
            let mut output_code = parse_input.clone();
            output_code.replace_range(Range::<usize>::from(range), formatted);
            output_code
        }
        _ => {
            let result = formatted.into_code();

            if !has_errors {
                check_reformat::check_reformat(CheckReformatParams {
                    root: &syntax,
                    text: &result,
                    source_type,
                    file_name,
                    format_context: context,
                });
            }

            result
        }
    };

    let formatted = formatted.replace(ROME_IGNORE, PRETTIER_IGNORE);

    let mut snapshot = String::new();

    writeln!(snapshot, "# Input").unwrap();
    writeln!(snapshot, "```js").unwrap();
    writeln!(snapshot, "{}", input_code).unwrap();
    writeln!(snapshot, "```").unwrap();
    writeln!(snapshot).unwrap();

    writeln!(snapshot, "# Output").unwrap();
    writeln!(snapshot, "```js").unwrap();
    writeln!(snapshot, "{}", formatted).unwrap();
    writeln!(snapshot, "```").unwrap();
    writeln!(snapshot).unwrap();

    if has_errors {
        let mut files = SimpleFiles::new();
        files.add(file_name.into(), parse_input);

        let mut buffer = termcolor::Buffer::no_color();
        let mut emitter = Emitter::new(&files);

        for error in parsed.diagnostics() {
            emitter
                .emit_with_writer(error, &mut buffer)
                .expect("failed to emit diagnostic");
        }

        writeln!(snapshot, "# Errors").unwrap();
        writeln!(snapshot, "```").unwrap();
        writeln!(
            snapshot,
            "{}",
            std::str::from_utf8(buffer.as_slice()).expect("non utf8 in error buffer")
        )
        .unwrap();
        writeln!(snapshot, "```").unwrap();
        writeln!(snapshot).unwrap();
    }

    let max_width = context.line_width.value() as usize;
    let mut lines_exceeding_max_width = formatted
        .lines()
        .enumerate()
        .filter(|(_, line)| line.len() > max_width)
        .peekable();

    if lines_exceeding_max_width.peek().is_some() {
        writeln!(
            snapshot,
            "# Lines exceeding max width of {max_width} characters"
        )
        .unwrap();
        writeln!(snapshot, "```").unwrap();

        for (index, line) in lines_exceeding_max_width {
            let line_number = index + 1;
            writeln!(snapshot, "{line_number:>5}: {line}").unwrap();
        }
        writeln!(snapshot, "```").unwrap();
    }

    insta::with_settings!({
        prepend_module_to_snapshot => false,
        snapshot_path => input_file.parent().unwrap(),
    }, {
        insta::assert_snapshot!(file_name, snapshot, file_name);
    });

    let snapshot_file = input_file
        .extension()
        .and_then(OsStr::to_str)
        .map(|ext| input_file.with_extension(format!("{}.prettier-snap", ext)))
        .filter(|path| path.exists());

    if let Some(snapshot_file) = snapshot_file {
        let mut content = read_to_string(snapshot_file).unwrap();

        strip_placeholders(&mut content);

        let root_path = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/specs/prettier/"
        ));

        let input_file = input_file.strip_prefix(root_path).unwrap_or_else(|_| {
            panic!(
                "failed to strip prefix {:?} from {:?}",
                root_path, input_file
            )
        });
        if formatted != content {
            let input_file = input_file.to_str().unwrap();
            DiffReport::get().report_diff(input_file, formatted, content);
        } else {
            let input_file = input_file.to_str().unwrap();
            DiffReport::get().report_match(input_file, formatted, content);
        }
    }
}

/// Find and replace the cursor, range start and range end placeholders in a
/// Prettier snapshot tests and return their indices in the resulting string
fn strip_placeholders(input_code: &mut String) -> (Option<usize>, Option<usize>, Option<usize>) {
    const CURSOR_PLACEHOLDER: &str = "<|>";
    const RANGE_START_PLACEHOLDER: &str = "<<<PRETTIER_RANGE_START>>>";
    const RANGE_END_PLACEHOLDER: &str = "<<<PRETTIER_RANGE_END>>>";

    let mut cursor_index = None;
    let mut range_start_index = None;
    let mut range_end_index = None;

    if let Some(index) = input_code.find(CURSOR_PLACEHOLDER) {
        input_code.replace_range(index..index + CURSOR_PLACEHOLDER.len(), "");
        cursor_index = Some(index);
    }

    if let Some(index) = input_code.find(RANGE_START_PLACEHOLDER) {
        input_code.replace_range(index..index + RANGE_START_PLACEHOLDER.len(), "");
        range_start_index = Some(index);

        if let Some(cursor) = &mut cursor_index {
            if *cursor > index {
                *cursor -= RANGE_START_PLACEHOLDER.len();
            }
        }
    }

    if let Some(index) = input_code.find(RANGE_END_PLACEHOLDER) {
        input_code.replace_range(index..index + RANGE_END_PLACEHOLDER.len(), "");
        range_end_index = Some(index);

        if let Some(cursor) = &mut cursor_index {
            if *cursor > index {
                *cursor -= RANGE_END_PLACEHOLDER.len();
            }
        }
        if let Some(cursor) = &mut range_start_index {
            // Prettier has tests for reversed ranges
            if *cursor > index {
                *cursor -= RANGE_END_PLACEHOLDER.len();
            }
        }
    }

    (cursor_index, range_start_index, range_end_index)
}

/// This enum type is used to represent if our formatting result is expected
/// [MatchCategory::Diff] means that our formatting result have some thing different from the expected
/// [MatchCategory::Match] means that our formatting result is the same as the expected
#[derive(Debug, PartialEq, Eq)]
enum MatchCategory {
    Diff,
    Match,
}

struct DiffReportItem {
    file_name: &'static str,
    rome_formatted_result: String,
    prettier_formatted_result: String,
    match_category: MatchCategory,
}
struct DiffReport {
    state: Mutex<Vec<DiffReportItem>>,
}

impl DiffReport {
    fn get() -> &'static Self {
        static REPORTER: DiffReport = DiffReport {
            state: const_mutex(Vec::new()),
        };

        // Use an atomic Once to register an exit callback the first time any
        // testing thread requests an instance of the Reporter
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            // Import the atexit function from libc
            extern "C" {
                fn atexit(f: extern "C" fn()) -> c_int;
            }

            // Trampoline function into the reporter printing logic with the
            // correct extern C ABI
            extern "C" fn print_report() {
                REPORTER.print();
            }

            // Register the print_report function to be called when the process exits
            unsafe {
                atexit(print_report);
            }
        });

        &REPORTER
    }

    fn report_diff(
        &self,
        file_name: &'static str,
        rome_formatted_result: String,
        prettier_formatted_result: String,
    ) {
        self.state.lock().push(DiffReportItem {
            file_name,
            rome_formatted_result,
            prettier_formatted_result,
            match_category: MatchCategory::Diff,
        });
    }

    fn report_match(
        &self,
        file_name: &'static str,
        rome_formatted_result: String,
        prettier_formatted_result: String,
    ) {
        self.state.lock().push(DiffReportItem {
            file_name,
            rome_formatted_result,
            prettier_formatted_result,
            match_category: MatchCategory::Match,
        });
    }
    fn print(&self) {
        if let Some(report) = rome_rowan::check_live() {
            panic!("\n{report}")
        }

        // Only create the report file if the REPORT_PRETTIER
        // environment variable is set to 1
        match env::var("REPORT_PRETTIER") {
            Ok(value) if value == "1" => {
                self.report_prettier();
            }
            _ => {}
        }
    }

    fn report_prettier(&self) {
        let mut report = String::new();
        let mut state = self.state.lock();
        state.sort_by_key(|DiffReportItem { file_name, .. }| *file_name);
        let mut sum_of_per_compatibility_file = 0_f64;
        let mut total_line = 0;
        let mut total_matched_line = 0;
        let mut file_count = 0;
        for DiffReportItem {
            file_name,
            rome_formatted_result,
            prettier_formatted_result,
            match_category,
        } in state.iter()
        {
            file_count += 1;
            let rome_lines = rome_formatted_result.lines().count();
            let prettier_lines = prettier_formatted_result.lines().count();
            let mut matched_lines = 0;
            let compatibility_per_file;
            if *match_category == MatchCategory::Diff {
                writeln!(report, "# {}", file_name).unwrap();
                writeln!(report, "```diff").unwrap();

                for (tag, line) in diff_lines(
                    Algorithm::default(),
                    prettier_formatted_result,
                    rome_formatted_result,
                ) {
                    if matches!(tag, ChangeTag::Equal) {
                        matched_lines += line.lines().count();
                    }
                    let line = line.strip_suffix('\n').unwrap_or(line);
                    writeln!(report, "{}{}", tag, line).unwrap();
                }

                compatibility_per_file =
                    matched_lines as f64 / rome_lines.max(prettier_lines) as f64;
                sum_of_per_compatibility_file += compatibility_per_file;
                writeln!(report, "```").unwrap();
                writeln!(report, "----").unwrap();
                writeln!(
                    report,
                    "**Prettier Similarity**: {:.2}%",
                    compatibility_per_file * 100_f64
                )
                .unwrap();
                writeln!(report).unwrap();
                writeln!(report, "----").unwrap();
            } else {
                // in this branch `rome_lines` == `prettier_lines` == `matched_lines`
                assert!(rome_lines == prettier_lines);
                matched_lines = rome_lines;
                sum_of_per_compatibility_file += 1_f64;
            }
            total_line += rome_lines.max(prettier_lines);
            total_matched_line += matched_lines;
        }
        // extra two space force markdown render insert a new line
        report = format!(
            "**File Based Average Prettier Similarity**: {:.2}%  \n**Line Based Average Prettier Similarity**: {:.2}%  \n the definition of similarity you could found here: https://github.com/rome/tools/issues/2555#issuecomment-1124787893 \n",
            (sum_of_per_compatibility_file / file_count as f64) * 100_f64,
            (total_matched_line as f64 / total_line as f64) * 100_f64
        ) + &report;
        write("report.md", report).unwrap();
    }
}
