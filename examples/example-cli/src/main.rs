/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use clap::{Parser, Subcommand};

use example_component::{ApiResult, ExampleComponent, TodoItem};

// Writing a CLI means using the `clap` library to define your CLI commands.
// The [clap documentation](https://docs.rs/clap/latest/clap/) is decent for this.
// Also, the other app-services CLI code can provide good examples

// Top-level clap CLI.  Each field corresponds to a CLI argument
#[derive(Debug, Parser)]
#[command(about, long_about = None)]
struct Cli {
    /// Enable verbose logging
    // Notes:
    //   * Docstrings show up in the CLI help
    //   * `short` means that this is associated with `-v`
    //   * `long` means that this is associated with `--verbose`
    //   * `action` means that this flag will set the boolean to `true`.
    #[arg(short, long, action)]
    verbose: bool,

    // Subcommands can be used to create git-like CLIs where the argument begins a new subcommand
    // and each subcommand can have different args
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Manage todo lists
    Lists {
        // Yes, sub-sub-commands are possible
        #[command(subcommand)]
        lists_command: Option<ListsCommands>,
    },
    /// Manage todo items
    Items {
        /// List to use
        list: String,
        #[command(subcommand)]
        items_command: Option<ItemsCommands>,
    },
}

#[derive(Debug, Subcommand)]
enum ListsCommands {
    /// List lists
    List,
    /// Create a new list
    Create {
        // Name is a position argument, since it doesn't have the `#[arg]` attribute.
        name: String,
    },
    /// delete a list
    Delete { name: String },
}

#[derive(Debug, Subcommand)]
enum ItemsCommands {
    /// List todos
    List,
    /// Create a new todo
    Add {
        // Name for the item
        name: String,
        #[arg(short, long)]
        github_issue: Option<String>,
    },
    /// Update a todo
    Update {
        // Name of the item to update
        name: String,
        #[arg(short, long)]
        description: Option<String>,
        #[arg(short, long)]
        url: Option<String>,
        #[arg(short, long, action)]
        toggle: bool,
    },
    /// Delete a todo
    Delete {
        // Name of the item to update
        name: String,
    },
}

fn main() -> ApiResult<()> {
    let cli = Cli::parse();
    init_logging(&cli);
    // Applications must initialize viaduct for the HTTP client to work.
    // This example uses the `reqwest` backend because it's easy to setup.
    viaduct_reqwest::use_reqwest_backend();
    let component = build_example_component()?;
    println!();
    match cli.command {
        Commands::Lists {
            lists_command: command,
        } => {
            let command = command.unwrap_or(ListsCommands::List);
            handle_lists(component, command)
        }
        Commands::Items {
            list,
            items_command: command,
        } => {
            let command = command.unwrap_or(ItemsCommands::List);
            handle_todos(component, list, command)
        }
    }
}

fn init_logging(cli: &Cli) {
    // The env_logger crate is a simple way to setup logging.
    //
    // This will enable trace-level logging if `-v` is present and `info-level` otherwise.
    let log_filter = if cli.verbose {
        "example_component=trace"
    } else {
        "example_component=info"
    };
    env_logger::init_from_env(env_logger::Env::default().filter_or("RUST_LOG", log_filter));
}

fn build_example_component() -> ApiResult<ExampleComponent> {
    // Use `cli_support` to get paths to store stuff in.  These will always be relative to
    // `[WORKSPACE_ROOT]/.cli-data`
    let db_path = cli_support::cli_data_path("example-component.db");
    ExampleComponent::new(&db_path)
}

fn handle_lists(component: ExampleComponent, subcommand: ListsCommands) -> ApiResult<()> {
    match subcommand {
        ListsCommands::List => {
            let lists = component.get_lists()?;
            if lists.is_empty() {
                println!("No lists created");
            } else {
                for list in lists {
                    println!("{}", list);
                }
            }
        }
        ListsCommands::Create { name } => {
            component.create_list(&name)?;
            println!("Created list: {name}");
        }
        ListsCommands::Delete { name } => {
            component.delete_list(&name)?;
            println!("Deleted list: {name}");
        }
    }
    Ok(())
}

fn handle_todos(
    component: ExampleComponent,
    list: String,
    subcommand: ItemsCommands,
) -> ApiResult<()> {
    match subcommand {
        ItemsCommands::List => {
            let items = component.get_list_items(&list)?;
            if items.is_empty() {
                println!("No items created");
            } else {
                println!("{:-^79}", format!(" {list} "));
                println!(
                    "{:<9} {:<29} {:<29} {:>9}",
                    "name", "description", "url", "completed"
                );
                for saved in items {
                    println!(
                        "{:<9} {:<29} {:<29} {:>9}",
                        clamp_string(&saved.item.name, 9),
                        clamp_string(&saved.item.description, 29),
                        clamp_string(&saved.item.url, 29),
                        if saved.item.completed { "X" } else { "" },
                    )
                }
            }
        }
        ItemsCommands::Add { name, github_issue } => {
            match github_issue {
                None => {
                    component.add_item(
                        &list,
                        TodoItem {
                            name: name.clone(),
                            ..TodoItem::default()
                        },
                    )?;
                    println!("Created item: {name}");
                }
                Some(github_issue) => {
                    component.add_item_from_gh_issue(&list, &name, &github_issue)?;
                    println!("Created item: {name} (from GH-{github_issue})");
                }
            };
        }
        ItemsCommands::Update {
            name,
            description,
            url,
            toggle,
        } => {
            let mut saved = component.get_list_item(&list, &name)?;
            if let Some(description) = description {
                saved.item.description = description;
            }
            if let Some(url) = url {
                saved.item.url = url;
            }
            if toggle {
                saved.item.completed = !saved.item.completed;
            }
            component.update_item(&saved)?;
            println!("Updated item: {name}");
        }
        ItemsCommands::Delete { name } => {
            let saved = component.get_list_item(&list, &name)?;
            component.delete_item(saved)?;
            println!("Deleted item: {name}");
        }
    }
    Ok(())
}

fn clamp_string(val: &str, max_width: usize) -> String {
    if val.len() > max_width {
        format!("{}...", &val[0..max_width - 3])
    } else {
        val.to_string()
    }
}
