//! Follow kkt connect's supplied client command to the concrete CLI operation.
use super::{Body, program, projection};

#[test]
fn connect_supplies_the_program_adapter_to_the_config_crate() {
    let dispatch = Body::open("kkt-bin", "run-{closure#0}");
    let build = dispatch.call("kashkeshet::machine::agent_parts::agent_parts");
    let run = dispatch.call("kkt_agent_commands::connect::run");
    assert_eq!(dispatch.origin(run.arg(0)), build.value);

    let connect = Body::open("kkt_agent_commands-lib", "connect-run");
    let configure = connect.call("kkt_llm_agent_config::configure");
    let field = projection(connect.origin(configure.arg(4)), "(*_1)");
    let builder = program("machine-agent_parts-agent_parts");
    let parts = builder.returned();
    let (name, value) = parts.field(field);
    assert_eq!(name, "client_command");
    let erased = builder.one("box as ClientCommand", |a| {
        Some(a.place) == value.strip_prefix("move ")
            && a.value
                .contains(" as std::boxed::Box<dyn kkt_llm_agent_config::ClientCommand> ")
    });
    let boxed = builder.method(
        "std::boxed::Box::<machine::client_config::ClaudeCommand>",
        "new",
    );
    assert_eq!(builder.origin(erased.value), boxed.value);
    assert_eq!(
        builder.origin(boxed.arg(0)),
        "machine::client_config::ClaudeCommand"
    );

    let config = Body::open("kkt_llm_agent_config-lib", "configure");
    let apply = config.call("files::apply_claude");
    assert_eq!(config.origin(apply.arg(1)), "_5");
    println!("  ClaudeCommand -> AgentParts.client_command -> configure -> apply_claude");
}

#[test]
fn both_contract_methods_delegate_to_the_corresponding_cli_operation() {
    let apply = Body::open("kkt_llm_agent_config-lib", "files-apply_claude");
    let invoke = apply.method("<{closure@", "call");
    let captured = apply.one("called closure", |a| {
        a.value.starts_with("{closure@") && a.value == apply.origin(invoke.arg(0))
    });
    let closure = Body::open("kkt_llm_agent_config-lib", "files-apply_claude-{closure#1}");
    let closure_type = captured.value.split_once(" { ").unwrap().0;
    assert!(
        closure
            .0
            .lines()
            .any(|line| line.starts_with("fn ") && line.contains(closure_type))
    );

    let provider = program("machine-client_config-CLAUDE");
    let new = provider.call("kkt_llm_agent_command::Claude::new");
    assert_eq!(new.place, "_0");

    for method in ["remove_server", "add_server"] {
        let call = closure.method(
            "<dyn command::ClientCommand as command::ClientCommand>",
            method,
        );
        let field = projection(closure.origin(call.arg(0)), "(*_1)");
        let (name, actual) = captured.field(field);
        assert_eq!((name, apply.origin(actual)), ("command", "_2"));

        let adapter = program(&format!("machine-client_config-{{impl#1}}-{method}"));
        assert!(adapter.0.lines().any(|line| line.starts_with("fn ")
            && line.contains(&format!(
                "::{method}(_1: &machine::client_config::ClaudeCommand,"
            ))));
        let delegates: Vec<_> = adapter
            .assignments()
            .filter(|a| {
                a.call().is_some_and(|(callee, _)| {
                    callee.starts_with("kkt_llm_agent_command::Claude::")
                })
            })
            .collect();
        // Reject a second CLI operation even if the expected operation is also present.
        assert_eq!(delegates.len(), 1);
        let underlying = adapter.call(&format!("kkt_llm_agent_command::Claude::{method}"));
        assert_eq!(
            adapter.origin(underlying.arg(0)),
            "const machine::client_config::CLAUDE"
        );
        let arity = if method == "add_server" { 3 } else { 2 };
        for index in 1..arity {
            assert_eq!(
                adapter.origin(underlying.arg(index)),
                format!("_{}", index + 1)
            );
        }
        let returned = adapter.returned();
        assert!(
            returned
                .call()
                .unwrap()
                .0
                .contains("::map_err::<kkt_llm_agent_config::CommandError,")
        );
        assert_eq!(adapter.origin(returned.arg(0)), underlying.value);
        assert_eq!(returned.arg(1), "machine::client_config::command_error");
        println!(
            "  ClientCommand::{method} -> ClaudeCommand -> Claude::{method} -> translate error"
        );
    }
}
