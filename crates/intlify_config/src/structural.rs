// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Private schema-guided admission for the minimum profile path. Test-owned
//! reference schemas are not formal 017 artifacts or construction authority.

mod eval;
mod program;

#[cfg(test)]
mod eval_tests;

#[cfg(test)]
pub(crate) fn test_schema_accepts(value: &serde_json::Value) -> bool {
    use crate::fixtures::FixtureConfig;
    use crate::input_limits::Bound;
    use std::sync::{Arc, OnceLock};
    static PROGRAM: OnceLock<program::SchemaProgram> = OnceLock::new();
    let program = PROGRAM.get_or_init(|| {
        program::SchemaProgram::compile(&crate::schema::draft7_schema::<FixtureConfig>().unwrap())
            .unwrap()
    });
    let raw = serde_json::to_vec(value).unwrap();
    let doc =
        crate::materialize::materialize_file(Arc::from(raw), crate::materialize_tests::limits())
            .unwrap();
    eval::evaluate(
        program,
        &doc,
        program.root(),
        doc.root(),
        Bound::new(1_000_000).unwrap(),
    )
    .unwrap()
    .admitted
}
