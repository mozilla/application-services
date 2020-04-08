#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HistoryVisitInfo {
    #[prost(string, required, tag="1")]
    pub url: std::string::String,
    #[prost(string, optional, tag="2")]
    pub title: ::std::option::Option<std::string::String>,
    #[prost(int64, required, tag="3")]
    pub timestamp: i64,
    #[prost(int32, required, tag="4")]
    pub visit_type: i32,
    #[prost(bool, required, tag="5")]
    pub is_hidden: bool,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HistoryVisitInfos {
    #[prost(message, repeated, tag="1")]
    pub infos: ::std::vec::Vec<HistoryVisitInfo>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HistoryVisitInfosWithBound {
    #[prost(message, repeated, tag="1")]
    pub infos: ::std::vec::Vec<HistoryVisitInfo>,
    #[prost(int64, required, tag="2")]
    pub bound: i64,
    #[prost(int64, required, tag="3")]
    pub offset: i64,
}
///*
/// A bookmark node.
///
/// We use a single message type for bookmarks. It covers insertion, deletion,
/// and update, and represents all three bookmark types.
///
/// This simplifies the FFI by reducing the number of types that must go across
/// it, and retuces boilderplate, but removes some static-ish guarantees we
/// might have otherwise.
///
/// Note that these docs comments are internal, and don't necessarily impact the actual
/// API we expose to Kotlin/Swift (this is particularly true around reads).
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BookmarkNode {
    ///*
    /// The type of this bookmark, a `BookmarkType` (from `types.rs`).
    ///
    /// This impacts which fields may be present.
    ///
    /// It's illegal to attempt to change this when updating a bookmark.
    ///
    /// Note: this probably should be an `enum`, but prost seems to get upset
    /// about it so we're just using int32 for now.
    ///
    /// Note: this is `node_type` and not `type` because `type` is reserved
    /// in Rust.
    ///
    /// - Always returned on reads.
    /// - Required for inserts.
    /// - Not provided for updates.
    #[prost(int32, optional, tag="1")]
    pub node_type: ::std::option::Option<i32>,
    ///*
    /// The bookmarks guid.
    ///
    /// - Always returned on reads.
    /// - Not allowed for inserts.
    /// - Required for updates (specifies which record is being changed)
    #[prost(string, optional, tag="2")]
    pub guid: ::std::option::Option<std::string::String>,
    ///*
    /// Creation time, in milliseconds since the unix epoch.
    ///
    /// May not be a local timestamp, and may shift if new devices are able to
    /// provide an earlier (but still valid) timestamp.
    ///
    /// - Always returned on reads.
    /// - Ignored for insertion and update.
    #[prost(int64, optional, tag="3")]
    pub date_added: ::std::option::Option<i64>,
    ///*
    /// Last modification time, in milliseconds since the unix epoch.
    ///
    /// - Always returned on reads.
    /// - Ignored for insertion and update.
    #[prost(int64, optional, tag="4")]
    pub last_modified: ::std::option::Option<i64>,
    ///*
    /// Guid of the parent record.
    ///
    /// - Returned on reads, except for reads of the bookmark root.
    /// - Required for insertion.
    /// - On updates, if provided, we treat it as a move.
    ///     - Interacts with `position`, see its documentation below
    ///       for details on how.
    #[prost(string, optional, tag="5")]
    pub parent_guid: ::std::option::Option<std::string::String>,
    ///*
    /// Zero based index within the parent.
    ///
    /// - Not provided on reads (for now).
    ///
    /// - Allowed for insertion.
    ///    - Leaving it out means 'end of folder'.
    ///
    /// - Allowed for updates.
    ///     - If `parent_guid` is not provided and `position` is, we treat this
    ///       a move within the same folder.
    ///
    ///     - If `parent_guid` and `position` are both provided, we treat this as
    ///       a move to / within that folder, and we insert at the requested
    ///       position.
    ///
    ///     - If `position` is not provided (and `parent_guid` is) then it's
    ///       treated as a move to the end of that folder.
    #[prost(uint32, optional, tag="6")]
    pub position: ::std::option::Option<u32>,
    ///*
    /// Bookmark title. Not present for type = `BookmarkType::Separator`.
    ///
    /// - Returned on reads if it exists.
    /// - Required when inserting folders.
    #[prost(string, optional, tag="7")]
    pub title: ::std::option::Option<std::string::String>,
    ///*
    /// Bookmark URL. Only allowed/present for type = `BookmarkType::Bookmark`.
    ///
    /// - Always returned on reads (for `BookmarkType::Bookmark`).
    /// - Required when inserting a new bookmark.
    #[prost(string, optional, tag="8")]
    pub url: ::std::option::Option<std::string::String>,
    ///*
    /// IDs of folder children, in order. Only present for type =
    /// `BookmarkType::Folder`.
    ///
    /// - Returned on reads (for `BookmarkType::Folder`).
    /// - Forbidden for insertions and updates.
    /// - Not provided if `child_nodes` is provided, to avoid sending more data
    ///   over the FFI than necessary.
    #[prost(string, repeated, tag="9")]
    pub child_guids: ::std::vec::Vec<std::string::String>,
    ///*
    /// Data about folder children, in order. Only present for type =
    /// `BookmarkType::Folder`.
    ///
    /// For performance reasons, this only is provided if it's requested.
    #[prost(message, repeated, tag="10")]
    pub child_nodes: ::std::vec::Vec<BookmarkNode>,
    ///*
    /// Returned by reads, and used to distinguish between the cases of
    /// "empty child_nodes because the API doesn't return children" and
    /// "empty child_nodes because this folder has no children (but
    /// we'd populate them if it had them)".
    ///
    /// Only required because you can't have `optional repeated`.
    ///
    /// Leaving this out is equivalent to false.
    #[prost(bool, optional, tag="11")]
    pub have_child_nodes: ::std::option::Option<bool>,
}
///* An array of bookmark nodes, since we can't represent that directly 
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BookmarkNodeList {
    #[prost(message, repeated, tag="1")]
    pub nodes: ::std::vec::Vec<BookmarkNode>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SearchResultMessage {
    #[prost(string, required, tag="1")]
    pub url: std::string::String,
    #[prost(string, required, tag="2")]
    pub title: std::string::String,
    #[prost(int64, required, tag="3")]
    pub frecency: i64,
    #[prost(enumeration="SearchResultReason", repeated, tag="4")]
    pub reasons: ::std::vec::Vec<i32>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SearchResultList {
    #[prost(message, repeated, tag="1")]
    pub results: ::std::vec::Vec<SearchResultMessage>,
}
/// Protobuf allows nesting these, but prost behaves weirdly if we do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum SearchResultReason {
    /// Never used in practice. Maybe remove this from here and from the rust enum?
    Keyword = 1,
    Origin = 2,
    Url = 3,
    PreviousUse = 4,
    Bookmark = 5,
    /// If we get real tag support, just add `optional string tags` to SearchResult below, but
    /// for now expose that it was because of tags.
    Tag = 6,
}
