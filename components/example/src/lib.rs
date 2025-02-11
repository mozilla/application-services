/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Top-level module, see README.md for an overview of what's going on in this component.
//!
//! `lib.rs` defines the main public API for the component.

mod db;
mod error;
mod http;
mod schema;

// Components use UniFFI to export Swift/Kotlin/JS bindings for their public API.
//
// * The [user guide](https://mozilla.github.io/uniffi-rs/latest/) gives an overview of UniFFI.
// * App-services is currently in the process to switching to the [proc-macro based approach](https://mozilla.github.io/uniffi-rs/latest/proc_macro/index.html), away from the UDL-based approach.
//
// If you have questions, ask on the `#application-services-eng` channel in Slack.
uniffi::setup_scaffolding!("example");

pub use db::{SavedTodoItem, TodoItem};
pub use error::{ApiError, ApiResult, Error, Result};

use error_support::handle_error;

/// Top-level API for the example component
#[derive(uniffi::Object)]
pub struct ExampleComponent {
    // This example splits `ExampleComponent` into a public and internal part.
    //
    // `ExampleComponent` provides the public API, while `ExampleComponentInner` implements the
    // internal API.  The two APIs are exactly the same, except `ExampleComponent` uses an
    // `ApiResult`/`ApiError` and `ExampleComponentInner` uses `Result`/`Error`. See `error.rs` for
    // a discussion of internal vs public results and errors.
    //
    //  The reason for this split is that `ExampleComponent` methods can't be called by other
    //  functions, since they all expect a `Result` rather than an `ApiResult`.  This extra layer
    //  requires writing extra boilerplate code, but it's usually worth it in the long-run.
    inner: ExampleComponentInner,
}

#[uniffi::export]
impl ExampleComponent {
    /// Construct a new [ExampleComponent]
    // Notes:
    //   * Triple-slash docstrings in the exported interface will appear in the public API references
    //     (https://firefox-source-docs.mozilla.org/rust-components/api/index.html)
    //   * The `#[handle_error(Error)]` macro automatically converts `Error` results from the
    //     function body to `ApiError` using the `GetErrorHandling` impl from `errors.rs.`
    #[uniffi::constructor]
    #[handle_error(Error)]
    pub fn new(path: &str) -> ApiResult<Self> {
        Ok(Self {
            inner: ExampleComponentInner::new(path)?,
        })
    }

    /// Get all todo lists
    #[handle_error(Error)]
    pub fn get_lists(&self) -> ApiResult<Vec<String>> {
        self.inner.get_lists()
    }

    /// Get todo lists that match a query
    #[handle_error(Error)]
    pub fn find_lists(&self, query: &str) -> ApiResult<Vec<String>> {
        self.inner.find_lists(query)
    }

    /// Create a new todo list
    #[handle_error(Error)]
    pub fn create_list(&self, name: &str) -> ApiResult<()> {
        self.inner.create_list(name)
    }

    /// Delete a todo list
    #[handle_error(Error)]
    pub fn delete_list(&self, name: &str) -> ApiResult<()> {
        self.inner.delete_list(name)
    }

    /// Get all items in a todo list
    #[handle_error(Error)]
    pub fn get_list_items(&self, list_name: &str) -> ApiResult<Vec<SavedTodoItem>> {
        self.inner.get_list_items(list_name)
    }

    /// Get a single item in a todo list
    #[handle_error(Error)]
    pub fn get_list_item(&self, list_name: &str, item_name: &str) -> ApiResult<SavedTodoItem> {
        self.inner.get_list_item(list_name, item_name)
    }

    /// Add an item to a todo list
    #[handle_error(Error)]
    pub fn add_item(&self, list_name: &str, item: TodoItem) -> ApiResult<SavedTodoItem> {
        self.inner.add_item(list_name, item)
    }

    /// Add an item using a GitHub issue for the initial description/url
    #[handle_error(Error)]
    pub fn add_item_from_gh_issue(
        &self,
        list_name: &str,
        name: &str,
        issue_id: &str,
    ) -> ApiResult<SavedTodoItem> {
        self.inner.add_item_from_gh_issue(list_name, name, issue_id)
    }

    /// Bulk-add items to a todo list
    #[handle_error(Error)]
    pub fn add_items(
        &self,
        list_name: &str,
        items: Vec<TodoItem>,
    ) -> ApiResult<Vec<SavedTodoItem>> {
        self.inner.add_items(list_name, items)
    }

    /// Update an item in a todo list
    #[handle_error(Error)]
    pub fn update_item(&self, saved_item: &SavedTodoItem) -> ApiResult<()> {
        self.inner.update_item(saved_item)
    }

    /// Delete an item from a todo list
    #[handle_error(Error)]
    pub fn delete_item(&self, saved_item: SavedTodoItem) -> ApiResult<()> {
        self.inner.delete_item(saved_item)
    }

    /// Shutdown the component
    ///
    /// This will interrupt all pending operations and allow the application to shutdown cleanly.
    pub fn shutdown(&self) {
        self.inner.interrupt_all()
    }

    /// Interrupt all current queries
    ///
    /// The main reason to use this is if you call `find_lists` many times in quick succession, for
    /// example as the user is typing in keys.  In that case, you probably want to call
    /// `interrupt_queries` when calling `find_lists` query to interrupt any previous calls.
    pub fn interrupt_queries(&self) {
        self.inner.interrupt_readers()
    }
}

struct ExampleComponentInner {
    dbs: db::Databases,
    http_client: http::HttpClient,
}

impl ExampleComponentInner {
    pub fn new(path: &str) -> Result<Self> {
        Ok(Self {
            dbs: db::Databases::new(path)?,
            http_client: http::HttpClient::new(),
        })
    }

    pub fn get_lists(&self) -> Result<Vec<String>> {
        self.dbs.read(|dao| dao.get_lists())
    }

    pub fn find_lists(&self, query: &str) -> Result<Vec<String>> {
        self.dbs.read(|dao| dao.find_lists(query))
    }

    pub fn create_list(&self, name: &str) -> Result<()> {
        self.dbs.write(|dao| dao.create_list(name))
    }

    pub fn delete_list(&self, name: &str) -> Result<()> {
        self.dbs.write(|dao| dao.delete_list(name))
    }

    pub fn get_list_items(&self, list_name: &str) -> Result<Vec<SavedTodoItem>> {
        self.dbs.read(|dao| dao.get_list_items(list_name))
    }

    pub fn get_list_item(&self, list_name: &str, item_name: &str) -> Result<SavedTodoItem> {
        self.dbs.read(|dao| dao.get_list_item(list_name, item_name))
    }

    pub fn add_item(&self, list_name: &str, item: TodoItem) -> Result<SavedTodoItem> {
        self.dbs.write(|dao| dao.add_item(list_name, item))
    }

    /// Add an item using a GitHub issue for the initial description/url
    pub fn add_item_from_gh_issue(
        &self,
        list_name: &str,
        name: &str,
        issue_id: &str,
    ) -> Result<SavedTodoItem> {
        self.add_item(
            list_name,
            self.http_client.fetch_todo_from_gh_issue(name, issue_id)?,
        )
    }

    pub fn add_items(&self, list_name: &str, items: Vec<TodoItem>) -> Result<Vec<SavedTodoItem>> {
        self.dbs.write(|dao| dao.add_items(list_name, items))
    }

    pub fn update_item(&self, saved_item: &SavedTodoItem) -> Result<()> {
        self.dbs.write(|dao| dao.update_item(saved_item))
    }

    pub fn delete_item(&self, saved_item: SavedTodoItem) -> Result<()> {
        self.dbs.write(|dao| dao.delete_item(saved_item))
    }

    pub fn interrupt_all(&self) {
        self.dbs.interrupt_all()
    }

    pub fn interrupt_readers(&self) {
        self.dbs.interrupt_readers()
    }
}
