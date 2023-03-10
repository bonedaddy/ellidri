use crate::{Command, MESSAGE_LENGTH};
use std::cell::RefCell;
use std::fmt;
use std::fmt::Write as _;

/// Helper to build an IRC message.
///
/// Use with `Buffer::message`.
pub struct MessageBuffer<'a> {
    buf: &'a mut String,
}

impl<'a> MessageBuffer<'a> {
    fn with_prefix(buf: &'a mut String, prefix: &str, command: impl Into<Command>) -> Self {
        if !prefix.is_empty() {
            buf.push(':');
            buf.push_str(prefix);
            buf.push(' ');
        }
        buf.push_str(command.into().as_str());
        MessageBuffer { buf }
    }

    /// Appends a parameter to the message.
    ///
    /// The parameter is trimmed before insertion.  If `param` is whitespace, it is not appended.
    ///
    /// **Note**: It is up to the caller to make sure there is no remaning whitespace or newline in
    /// the parameter.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use ellidri_tokens::{Command, Buffer};
    /// let mut response = Buffer::new();
    ///
    /// response.message("nick!user@127.0.0.1", Command::Quit)
    ///     .param("")
    ///     .param("  chiao ");
    ///
    /// assert_eq!(&response.build(), ":nick!user@127.0.0.1 QUIT chiao\r\n");
    /// ```
    pub fn param(self, param: &str) -> Self {
        let param = param.trim();
        if param.is_empty() {
            return self;
        }
        self.buf.push(' ');
        self.buf.push_str(param);
        self
    }

    /// Formats, then appends a parameter to the message.
    ///
    /// The parameter is **NOT** trimmed before insertion, is appended even if it's empty.  Use
    /// `Buffer::param` to append strings, especially untrusted ones.
    ///
    /// **Note**: It is up to the caller to make sure there is no remaning whitespace or newline in
    /// the parameter.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use ellidri_tokens::{Command, Buffer};
    /// let mut response = Buffer::new();
    ///
    /// response.message("", Command::PrivMsg)
    ///     .fmt_param("  #space ")
    ///     .fmt_param(42);
    ///
    /// assert_eq!(&response.build(), "PRIVMSG   #space  42\r\n");
    /// ```
    pub fn fmt_param(self, param: impl fmt::Display) -> Self {
        let _ = write!(self.buf, " {param}");
        self
    }

    pub fn raw_param(&mut self) -> &mut String {
        self.buf.push(' ');
        self.buf
    }

    /// Appends the traililng parameter to the message and consumes the buffer.
    ///
    /// Contrary to `MessageBuffer::param`, the parameter is not trimmed before insertion.  Even if
    /// `param` is just whitespace, it is appended.
    ///
    /// **Note**: It is up to the caller to make sure there is no newline in the parameter.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use ellidri_tokens::{Command, Buffer};
    /// let mut response = Buffer::new();
    ///
    /// response.message("nick!user@127.0.0.1", Command::Quit)
    ///     .trailing_param("long quit message");
    ///
    /// assert_eq!(&response.build(), ":nick!user@127.0.0.1 QUIT :long quit message\r\n");
    /// ```
    pub fn trailing_param(self, param: &str) {
        self.buf.push(' ');
        self.buf.push(':');
        self.buf.push_str(param);
    }

    pub fn fmt_trailing_param(self, param: impl fmt::Display) {
        let _ = write!(self.buf, " :{param}");
    }

    pub fn raw_trailing_param(&mut self) -> &mut String {
        self.buf.push(' ');
        self.buf.push(':');
        self.buf
    }
}

impl Drop for MessageBuffer<'_> {
    /// Auto-magically append "\r\n" when the `MessageBuffer` is dropped.
    fn drop(&mut self) {
        self.buf.push('\r');
        self.buf.push('\n');
    }
}

thread_local! {
    static UNESCAPED_VALUE: RefCell<String> = RefCell::new(String::new());
}

fn write_escaped(buf: &mut String, value: impl fmt::Display) {
    UNESCAPED_VALUE.with(|s| {
        let mut s = s.borrow_mut();

        s.clear();
        let _ = write!(s, "{value}");

        buf.reserve(s.len());
        for c in s.chars() {
            match c {
                ';' => buf.push_str("\\:"),
                ' ' => buf.push_str("\\s"),
                '\r' => buf.push_str("\\r"),
                '\n' => buf.push_str("\\n"),
                '\\' => buf.push_str("\\\\"),
                c => buf.push(c),
            }
        }
    });
}

/// Helper to build the tags of an IRC message.
pub struct TagBuffer<'a> {
    buf: &'a mut String,
    tag_start: usize,
}

impl<'a> TagBuffer<'a> {
    /// Creates a new tag buffer.  This function is private, because it is meant to be called by
    /// `Buffer`.
    fn new(buf: &'a mut String) -> Self {
        buf.reserve(MESSAGE_LENGTH);
        let tag_start = buf.len();
        buf.push('@');
        TagBuffer { buf, tag_start }
    }

    /// Whether the buffer has tags in it or not.
    pub fn is_empty(&self) -> bool {
        self.buf.len() == self.tag_start + 1
    }

    /// Adds a new tag to the buffer, with the given `key` and `value`.
    pub fn tag(self, key: &str, value: Option<impl fmt::Display>) -> Self {
        if !self.is_empty() {
            self.buf.push(';');
        }
        self.buf.push_str(key);
        if let Some(value) = value {
            self.buf.push('=');
            write_escaped(self.buf, value);
        }
        self
    }

    /// Adds the tag string `s`.
    fn raw_tag(self, s: &str) -> Self {
        if !self.is_empty() {
            self.buf.push(';');
        }
        self.buf.push_str(s);
        self
    }

    /// Writes the length of tags in `out`.
    ///
    /// Use this to know the start of the prefix or command.
    pub fn save_tag_len(self, out: &mut usize) -> Self {
        if self.buf.ends_with('@') {
            *out = 0;
        } else {
            *out = self.buf.len() + 1 - self.tag_start;
        }
        self
    }

    /// Starts building a message with the given prefix and command.
    pub fn prefixed_command(self, prefix: &str, cmd: impl Into<Command>) -> MessageBuffer<'a> {
        if self.is_empty() {
            self.buf.pop();
        } else {
            self.buf.push(' ');
        }
        MessageBuffer::with_prefix(self.buf, prefix, cmd)
    }
}

/// Helper to build IRC messages.
///
/// The `Buffer` is used to ease the creation of strings representing valid IRC messages.
///
/// # Example
///
/// ```rust
/// # use ellidri_tokens::{Command, Buffer, rpl};
/// let mut response = Buffer::new();
///
/// response.message("nick!user@127.0.0.1", Command::Topic)
///     .param("#hall")
///     .trailing_param("Welcome to new users!");
/// response.message("ellidri.dev", rpl::TOPIC)
///     .param("nickname")
///     .param("#hall")
///     .trailing_param("Welcome to new users!");
///
/// let result = response.build();
/// assert_eq!(&result, ":nick!user@127.0.0.1 TOPIC #hall :Welcome to new users!\r\n\
/// :ellidri.dev 332 nickname #hall :Welcome to new users!\r\n");
/// ```
#[derive(Debug)]
pub struct Buffer {
    buf: String,
}

impl Default for Buffer {
    fn default() -> Self {
        Self::new()
    }
}

impl From<String> for Buffer {
    fn from(val: String) -> Self {
        Self { buf: val }
    }
}

impl Buffer {
    /// Creates a `Buffer`.  Does not allocate.
    pub fn new() -> Self {
        Self { buf: String::new() }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buf: String::with_capacity(capacity),
        }
    }

    /// Whether the buffer is empty.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use ellidri_tokens::{Command, Buffer};
    /// let empty = Buffer::new();
    /// let mut not_empty = Buffer::new();
    ///
    /// not_empty.message("ellidri.dev", Command::Motd);
    ///
    /// assert_eq!(empty.is_empty(), true);
    /// assert_eq!(not_empty.is_empty(), false);
    /// ```
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Returns a reference to the underlying `String`.
    pub fn get(&self) -> &str {
        &self.buf
    }

    /// Empties the buffer.
    pub fn clear(&mut self) {
        self.buf.clear();
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn capacity(&self) -> usize {
        self.buf.capacity()
    }

    pub fn reserve(&mut self, capacity: usize) {
        self.buf.reserve(capacity);
    }

    /// Appends an IRC message with a prefix to the buffer.
    ///
    /// This function may allocate to reserve space for the message.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use ellidri_tokens::{Command, Buffer};
    /// let mut response = Buffer::new();
    ///
    /// response.message("unneeded_prefix", Command::Admin);
    ///
    /// assert_eq!(&response.build(), ":unneeded_prefix ADMIN\r\n");
    /// ```
    pub fn message(&mut self, prefix: &str, command: impl Into<Command>) -> MessageBuffer<'_> {
        MessageBuffer::with_prefix(&mut self.buf, prefix, command)
    }

    /// Start building an IRC message with tags.
    ///
    /// Server tags are filtered from `client_tags`, so that only tags with the client prefix `+`
    /// are appended to the buffer.
    ///
    /// The length of the resulting tags (`@` and ` ` included) is written to `tags_len`.
    ///
    /// TODO example
    pub fn tagged_message(&mut self, client_tags: &str) -> TagBuffer<'_> {
        client_tags
            .split(';')
            .filter(|s| s.starts_with('+') && !s.starts_with("+="))
            .fold(TagBuffer::new(&mut self.buf), |buf, tag| buf.raw_tag(tag))
    }

    /// Consumes the `Buffer` and returns the underlying `String`.
    pub fn build(self) -> String {
        self.buf
    }
}

thread_local! {
    static DOMAIN: RefCell<String> = RefCell::new(String::with_capacity(128));
    static NICKNAME: RefCell<String> = RefCell::new(String::with_capacity(64));
    static LABEL: RefCell<String> = RefCell::new(String::with_capacity(64));
}

pub struct ReplyBuffer {
    buf: Buffer,
    batch: Option<usize>,
    has_label: bool,
}

impl ReplyBuffer {
    pub fn new(domain: &str, nickname: &str, label: &str) -> Self {
        Self::set_nick(nickname);
        Self::set_domain(domain);
        Self::set_label(label);
        Self {
            buf: Buffer::new(),
            batch: None,
            has_label: !label.is_empty(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn tagged_message(&mut self, tags: &str) -> TagBuffer<'_> {
        self.buf.reserve(crate::MESSAGE_LENGTH);
        let mut msg = self.buf.tagged_message(tags);

        if self.has_label {
            self.has_label = false;
            msg = LABEL.with(|s| msg.tag("label", Some(&s.borrow())));
        }
        if let Some(batch) = self.batch {
            msg = msg.tag("batch", Some(&batch));
        }

        msg
    }

    pub fn message(&mut self, prefix: &str, command: impl Into<Command>) -> MessageBuffer<'_> {
        self.tagged_message("").prefixed_command(prefix, command)
    }

    pub fn prefixed_message(&mut self, command: impl Into<Command>) -> MessageBuffer<'_> {
        DOMAIN.with(move |s| self.message(&s.borrow(), command))
    }

    pub fn reply(&mut self, r: impl Into<Command>) -> MessageBuffer<'_> {
        NICKNAME.with(move |s| self.prefixed_message(r).param(&s.borrow()))
    }

    pub fn lr_batch_begin(&mut self) {
        if !self.has_label {
            return;
        }
        self.has_label = false;

        let new_batch = self.new_batch();
        LABEL.with(|label| {
            DOMAIN.with(|domain| {
                let label = label.borrow();
                let domain = domain.borrow();

                self.buf
                    .tagged_message("")
                    .tag("label", Some(&label))
                    .prefixed_command(&domain, "BATCH")
                    .fmt_param(format_args!("+{new_batch}"))
                    .param("labeled-response");
            })
        });
    }

    pub fn lr_end(&mut self) {
        if !self.has_label && self.batch.is_none() {
            return;
        }
        if self.batch.is_some() {
            self.batch_end();
        }
        if self.batch.is_some() {
            panic!("ReplyBuffer has an ongoing batch after the end of a labeled response");
        }
        if self.is_empty() {
            self.prefixed_message("ACK");
        }
        self.has_label = false;
    }

    pub fn batch_begin(&mut self, name: &str) {
        let new_batch = self.new_batch();
        self.prefixed_message("BATCH")
            .fmt_param(format_args!("+{new_batch}"))
            .param(name);
    }

    pub fn batch_end(&mut self) {
        let prev = match self.batch {
            Some(prev) => prev,
            None => return,
        };
        self.batch = if prev == 0 { None } else { Some(prev - 1) };
        self.prefixed_message("BATCH")
            .fmt_param(format_args!("-{prev}"));
    }

    pub fn build(self) -> String {
        self.buf.build()
    }

    pub fn set_nick(nickname: &str) {
        NICKNAME.with(|s| {
            let mut s = s.borrow_mut();
            s.clear();
            s.push_str(nickname);
        });
    }

    fn set_domain(domain: &str) {
        DOMAIN.with(|s| {
            let mut s = s.borrow_mut();
            s.clear();
            s.push_str(domain);
        });
    }

    fn set_label(label: &str) {
        if label.is_empty() {
            return;
        }
        LABEL.with(|s| {
            let mut s = s.borrow_mut();
            s.clear();
            s.push_str(label);
        });
    }

    fn new_batch(&mut self) -> usize {
        let next = self.batch.map_or(0, |prev| prev + 1);
        self.batch = Some(next);
        next
    }
}
