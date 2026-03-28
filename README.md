# tree-hoprs

A powerful CLI tool to manage multiple git repositories and worktrees efficiently. Tree-hoprs simplifies working with git worktrees by providing an intuitive interface to create, list, update, and manage multiple worktrees across different repositories.

## What it does

Tree-hoprs is a git worktree management tool that helps developers:

- **Create worktrees** - Quickly create new git worktrees for different branches without switching contexts
- **List worktrees** - View all active worktrees with their paths and branch names
- **Update worktrees** - Keep your main worktree synchronized with the latest changes
- **Delete worktrees** - Archive and remove worktrees when they're no longer needed
- **Manage repositories** - Add, remove, and switch between multiple repositories
- **Interactive UI** - Use an intuitive terminal UI for easy navigation (default mode when no command is specified)
- **Multiple repositories** - Support for managing multiple git repositories from a single tool

## Features

| Feature | Description | Status |
| --- | --- | --- |
| Interactive UI | Terminal-based UI for managing worktrees | ✅ |
| Create worktrees | Create a new worktree for a branch | ✅ |
| Delete worktrees | Archive and delete worktrees | ✅ |
| Update worktree | Update the main worktree | ✅ |
| List worktrees | List all worktrees with details | ✅ |
| Switch repositories | Switch between configured repositories | ✅ |
| Repository management | Add, remove, and list repositories | ✅ |
| Dry-run mode | Preview commands without executing them | ✅ |
| Verbose output | Detailed logging for debugging | ✅ |
| Autocomplete | Autocomplete for commands | :x: |

## Installation

```bash
cargo install --path .
```

## Usage

### Interactive Mode (Recommended)

Simply run the tool without any arguments to launch the interactive terminal UI:

```bash
tree-hoprs
```

### Command Line Mode

#### List worktrees

```bash
# Display worktrees in a formatted table
tree-hoprs list

# Display worktrees as raw output (one per line)
tree-hoprs list --raw
```

#### Create a new worktree

```bash
tree-hoprs create <branch-name>
```

Example:
```bash
tree-hoprs create feature/new-feature
```

#### Delete worktrees

```bash
tree-hoprs delete <branch-name> [additional-branch-names]
```

Example:
```bash
tree-hoprs delete feature/old-feature
tree-hoprs delete feature/done-1 feature/done-2
```

#### Update main worktree

```bash
tree-hoprs update
```

#### Repository Management

Add a new repository:
```bash
tree-hoprs add-repo <repo-name> <base-tree> <base-path>
```

Example:
```bash
tree-hoprs add-repo my-project main /home/user/projects/my-project
```

Set active repository:
```bash
tree-hoprs set-repo <repo-name>
```

List all repositories:
```bash
tree-hoprs get-repos
```

Delete a repository:
```bash
tree-hoprs delete-repo <repo-name>
```

#### Additional Options

- `--verbose` or `-v` - Enable verbose output for debugging
- `--dry-run` or `-d` - Preview commands without executing them
- `--repo` or `-r <repo-name>` - Specify which repository to operate on
- `--help` - Display help information

## Example Workflow

```bash
# Launch interactive UI
tree-hoprs

# Or use command line:
# Add a repository
tree-hoprs add-repo my-project main /path/to/repo

# Create a new worktree for a feature branch
tree-hoprs create feature/user-auth

# List all worktrees
tree-hoprs list

# Update the main worktree
tree-hoprs update

# Delete the worktree when done
tree-hoprs delete feature/user-auth
```

## Configuration

Tree-hoprs stores its configuration in a config file. Repositories and their settings are managed through the CLI commands or the interactive UI.

## Help

For more information on any command:

```bash
tree-hoprs --help
```
