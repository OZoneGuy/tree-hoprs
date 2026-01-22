complete -c tree-hoprs -f

# Commands
complete -c tree-hoprs -n "__fish_use_subcommand" -a "list" -d "List all the worktrees"
complete -c tree-hoprs -n "__fish_use_subcommand" -a "create" -d "Create a new worktree"
complete -c tree-hoprs -n "__fish_use_subcommand" -a "delete" -d "Archive a worktree"
complete -c tree-hoprs -n "__fish_use_subcommand" -a "update" -d "Update a worktree"
complete -c tree-hoprs -n "__fish_use_subcommand" -a "set-repo" -d "Set the active repo"
complete -c tree-hoprs -n "__fish_use_subcommand" -a "get-repos" -d "Get the repos list"
complete -c tree-hoprs -n "__fish_use_subcommand" -a "delete-repo" -d "Delete a repo"
complete -c tree-hoprs -n "__fish_use_subcommand" -a "add-repo" -d "Add a repo"
complete -c tree-hoprs -n "__fish_use_subcommand" -a "help" -d "Print this message or the help of the given subcommand(s)"

# Global options
complete -c tree-hoprs -s v -l verbose -d "Verbose output"
complete -c tree-hoprs -s d -l dry-run -d "Don't actually do anything, just print the commands"
complete -c tree-hoprs -s b -l base -d "The base branch to use" -r
complete -c tree-hoprs -s p -l path -d "The base path to use" -r
complete -c tree-hoprs -s r -l repo -d "The config file to use" -r
complete -c tree-hoprs -s h -l help -d "Print help"
complete -c tree-hoprs -s V -l version -d "Print version"

complete -c tree-hoprs -n "__fish_seen_subcommand_from delete" -a "(tree-hoprs list -r | string match -v main)"
complete -c tree-hoprs -n "__fish_seen_subcommand_from set-repo" -a "(tree-hoprs get-repos)"
