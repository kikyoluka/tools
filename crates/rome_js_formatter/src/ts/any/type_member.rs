//! Generated file, do not edit by hand, see `xtask/codegen`

use crate::generated::FormatTsAnyTypeMember;
use crate::prelude::*;
use rome_js_syntax::TsAnyTypeMember;
impl FormatRule<TsAnyTypeMember> for FormatTsAnyTypeMember {
    type Context = JsFormatContext;
    fn fmt(node: &TsAnyTypeMember, f: &mut JsFormatter) -> FormatResult<()> {
        match node {
            TsAnyTypeMember::TsCallSignatureTypeMember(node) => node.format().fmt(f),
            TsAnyTypeMember::TsPropertySignatureTypeMember(node) => node.format().fmt(f),
            TsAnyTypeMember::TsConstructSignatureTypeMember(node) => node.format().fmt(f),
            TsAnyTypeMember::TsMethodSignatureTypeMember(node) => node.format().fmt(f),
            TsAnyTypeMember::TsGetterSignatureTypeMember(node) => node.format().fmt(f),
            TsAnyTypeMember::TsSetterSignatureTypeMember(node) => node.format().fmt(f),
            TsAnyTypeMember::TsIndexSignatureTypeMember(node) => node.format().fmt(f),
            TsAnyTypeMember::JsUnknownMember(node) => node.format().fmt(f),
        }
    }
}
