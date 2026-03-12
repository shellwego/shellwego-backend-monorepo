//! Shell completion generators

/// Generate shell completion script
pub fn generate(shell: &str) -> String {
    match shell {
        "bash" => generate_bash(),
        "zsh" => generate_zsh(),
        "fish" => generate_fish(),
        "powershell" => generate_powershell(),
        _ => panic!("Unknown shell: {}", shell),
    }
}

fn generate_bash() -> String {
    // TODO: Generate bash completion script using clap_complete
    r#"
_shellwego_completions() {
    local cur prev opts
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"
    opts="apps nodes volumes domains status help"
    
    if [[ ${cur} == -* ]]; then
        COMPREPLY=( $(compgen -W "--help --version" -- ${cur}) )
        return 0
    fi
    
    COMPREPLY=( $(compgen -W "${opts}" -- ${cur}) )
    return 0
}
complete -F _shellwego_completions shellwego
"#.to_string()
}

fn generate_zsh() -> String {
    // TODO: Generate zsh completion
    "#compdef shellwego\n# TODO: ZSH completion".to_string()
}

fn generate_fish() -> String {
    // TODO: Generate fish completion
    "complete -c shellwego -f".to_string()
}

fn generate_powershell() -> String {
    // TODO: Generate PowerShell completion
    "# TODO: PowerShell completion".to_string()
}