use tiny_dsl::{
    Aggregate, Capture, ComparisonValue, Group, Relation, RelationTarget, Selection, Severity,
    TextOperator, WhereClause, parse,
};

#[test]
fn requires_the_hash_prefixed_version_header() {
    let document = parse("#badbox 1\n").expect("hash-prefixed header should parse");
    assert_eq!(document.version, 1);

    let error = parse("badbox 1\n").expect_err("plain header should be rejected");
    assert!(error.to_string().contains("#badbox 1"));
}

#[test]
fn parses_versioned_rules_and_external_fixture_tests() {
    let document = parse(
        r#"#badbox 1

rule rust/excessive-clones for rust {
  summary "Excessive clone calls within one callable"
  param limit = 4
  find code(value) `value.clone()`
  group by nearest callable
  when count > limit
  report {
    severity warning
    message "Callable contains too many clone call sites"
    evidence "clone call sites"
  }
}

rule go/excessive-goroutines for go {
  summary "Excessive goroutine launch sites"
  find node go_statement
  group by nearest node(function_declaration, method_declaration, func_literal)
  when count > 1
  report {
    severity info
    message "Callable contains too many goroutine launch sites"
    evidence "goroutine launch sites"
  }
}

test rust/excessive-clones "reports excessive clones" {
  input "fixtures/excessive.rs"
  expect {
    findings 1
    count 5
  }
}

test rust/excessive-clones "accepts limited clones" {
  input "fixtures/acceptable.rs"
  expect findings 0
}
"#,
    )
    .expect("document parses");

    assert_eq!(document.version, 1);
    assert_eq!(document.rules.len(), 2);
    assert_eq!(document.tests.len(), 2);

    let rust = &document.rules[0];
    assert_eq!(rust.id, "rust/excessive-clones");
    assert_eq!(rust.language, "rust");
    assert_eq!(rust.parameters[0].name, "limit");
    assert_eq!(rust.parameters[0].value, ComparisonValue::Integer(4));
    assert_eq!(
        rust.selection,
        Selection::Code {
            captures: vec![Capture {
                name: "value".into(),
                multiple: false,
            }],
            template: "value.clone()".into(),
        }
    );
    assert_eq!(rust.group, Group::Callable);
    assert_eq!(rust.condition.aggregate, Aggregate::Count);
    assert_eq!(
        rust.condition.value,
        ComparisonValue::Parameter("limit".into())
    );
    assert_eq!(rust.report.severity, Severity::Warning);

    let go = &document.rules[1];
    assert_eq!(go.selection, Selection::Node("go_statement".into()));
    assert_eq!(
        go.group,
        Group::Node(vec![
            "function_declaration".into(),
            "method_declaration".into(),
            "func_literal".into(),
        ])
    );
    assert_eq!(document.tests[0].expect.findings, 1);
    assert_eq!(document.tests[0].expect.count, Some(5));
    assert_eq!(document.tests[1].expect.findings, 0);
}

#[test]
fn parses_relational_and_capture_predicates_without_erasing_them() {
    let document = parse(
        r#"#badbox 1
rule rust/checked-calls for rust {
  summary "Calls should have recognized evidence"
  find code(value, method) `value.method()`
  group by nearest callable
  where text(method) in ["unwrap", "expect"]
  where group lacks any {
    code(ctx) `ctx.cancel()`
    node select_expression
  }
  when count > 0
  report {
    severity info
    message "No recognized evidence was found"
    evidence "unchecked call sites"
  }
}
"#,
    )
    .expect("document parses");

    assert_eq!(document.rules[0].where_clauses.len(), 2);
    assert_eq!(
        document.rules[0].where_clauses[0],
        WhereClause::Text {
            capture: "method".into(),
            operator: TextOperator::In,
            values: vec!["unwrap".into(), "expect".into()],
        }
    );
    assert_eq!(
        document.rules[0].where_clauses[1],
        WhereClause::Relation {
            target: RelationTarget::Group,
            relation: Relation::Lacks,
            require_all: false,
            selections: vec![
                Selection::Code {
                    captures: vec![Capture {
                        name: "ctx".into(),
                        multiple: false,
                    }],
                    template: "ctx.cancel()".into(),
                },
                Selection::Node("select_expression".into()),
            ],
        }
    );
}

#[test]
fn rejects_unsupported_versions_and_invalid_rule_ids() {
    let unsupported = parse("#badbox 2").expect_err("version is rejected");
    assert!(
        unsupported
            .to_string()
            .contains("unsupported Badbox syntax version")
    );

    let invalid = parse(
        r#"#badbox 1
rule NoNamespace for rust {}
"#,
    )
    .expect_err("ID is rejected");
    assert!(invalid.to_string().contains("lowercase namespace/name"));
}
