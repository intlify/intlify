// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReservedCommand {
    pub(crate) name: &'static str,
    // NOTE: Temporary Phase 3A metadata for reserved-command diagnostics;
    // remove this field once the real command implementations make phase hints
    // obsolete.
    pub(crate) required_phase: &'static str,
    pub(crate) requires: &'static [&'static str],
}

pub(crate) fn reserved_command(name: &'static str) -> ReservedCommand {
    match name {
        "fmt" => ReservedCommand {
            name,
            required_phase: "3B",
            requires: &[],
        },
        "lint" => ReservedCommand {
            name,
            required_phase: "3C",
            requires: &[],
        },
        "check" | "init" => ReservedCommand {
            name,
            required_phase: "3B+3C",
            requires: &["fmt", "lint"],
        },
        _ => unreachable!("reserved command names are parsed before routing"),
    }
}

pub(crate) fn top_level_help() -> String {
    concat!(
        "Usage: intlify [options]\n",
        "\n",
        "Options:\n",
        "  -h, --help                 Show help.\n",
        "  -V, --version              Show version.\n",
        "      --config <path>        Use an explicit project config path.\n",
        "      --reporter <text|json> Select the output reporter.\n"
    )
    .to_owned()
}

pub(crate) fn reserved_help(command: &str) -> String {
    let extra = if command == "init" {
        "\nThis command is reserved for future config scaffolding.\n"
    } else {
        "\n"
    };

    format!(
        "Usage: intlify {command} [options]\n\nThe {command} command is reserved but not available in this release.{extra}"
    )
}
