use crate::generated::FormatJsxChildList;
use crate::prelude::*;
use crate::utils::jsx_utils::contains_meaningful_jsx_text;
use crate::JsFormatter;
use rome_js_syntax::JsxChildList;

impl FormatRule<JsxChildList> for FormatJsxChildList {
    type Context = JsFormatContext;

    fn fmt(node: &JsxChildList, formatter: &mut JsFormatter) -> FormatResult<()> {
        if contains_meaningful_jsx_text(node) {
            formatter
                .fill(soft_line_break())
                .flatten_entries(node.iter().formatted())
                .finish()
        } else {
            formatter
                .join_with(soft_line_break())
                .entries(node.iter().formatted())
                .finish()
        }
    }
}
