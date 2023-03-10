use crate::data::modes;
use crate::util;
use ellidri_tokens::{mode, rpl, MessageBuffer};
use std::collections::HashMap;

/// Modes applied to clients on a per-channel basis.
///
/// <https://tools.ietf.org/html/rfc2811.html#section-4.1>
#[derive(Clone, Copy, Default)]
pub struct MemberModes {
    pub founder: bool,
    pub protected: bool,
    pub operator: bool,
    pub halfop: bool,
    pub voice: bool,
}

impl MemberModes {
    /// Pushes all the modes' symbols to the given string, in decreasing order of rank.
    pub fn all_symbols(self, out: &mut String) {
        if self.founder {
            out.push('~');
        }
        if self.protected {
            out.push('&');
        }
        if self.operator {
            out.push('@');
        }
        if self.halfop {
            out.push('%');
        }
        if self.voice {
            out.push('+');
        }
    }

    /// Returns the highest enabled mode.
    pub fn symbol(self) -> Option<char> {
        if self.founder {
            Some('~')
        } else if self.protected {
            Some('&')
        } else if self.operator {
            Some('@')
        } else if self.halfop {
            Some('%')
        } else if self.voice {
            Some('+')
        } else {
            None
        }
    }

    pub fn is_at_least_op(self) -> bool {
        self.operator || self.founder
    }

    pub fn is_at_least_halfop(self) -> bool {
        self.halfop || self.operator || self.founder
    }

    pub fn has_voice(self) -> bool {
        self.voice || self.halfop || self.operator || self.protected || self.founder
    }

    pub fn can_change(self, modes: modes::Channel<'_>) -> bool {
        use mode::ChannelChange::*;

        modes.iter().all(|mode| match mode {
            Err(_) => true,
            Ok(GetBans) | Ok(GetExceptions) | Ok(GetInvitations) => true,
            Ok(Moderated(_))
            | Ok(TopicRestricted(_))
            | Ok(UserLimit(_))
            | Ok(ChangeBan(_, _))
            | Ok(ChangeException(_, _))
            | Ok(ChangeInvitation(_, _))
            | Ok(ChangeVoice(_, _)) => self.is_at_least_halfop(),
            Ok(InviteOnly(_))
            | Ok(NoPrivMsgFromOutside(_))
            | Ok(Secret(_))
            | Ok(Key(_, _))
            | Ok(ChangeOperator(_, _))
            | Ok(ChangeHalfop(_, _)) => self.is_at_least_op(),
        })
    }
}

pub struct Topic {
    pub content: String,
    pub who: String,
    pub time: u64,
}

/// Channel data.
pub struct Channel {
    /// Set of channel members, identified by their socket address, and associated with their
    /// channel mode.
    pub members: HashMap<usize, MemberModes>,

    /// The topic.
    pub topic: Option<Topic>,

    pub user_limit: Option<usize>,
    pub key: Option<String>,

    // https://tools.ietf.org/html/rfc2811.html#section-4.3
    pub ban_mask: util::MaskSet,
    pub exception_mask: util::MaskSet,
    pub invex_mask: util::MaskSet,

    // Modes: https://tools.ietf.org/html/rfc2811.html#section-4.2
    pub invite_only: bool,
    pub moderated: bool,
    pub no_msg_from_outside: bool,
    pub secret: bool,
    pub topic_restricted: bool,
}

impl Channel {
    /// Creates a channel with the given modes set.
    ///
    /// # Panics
    ///
    /// This function panics when `modes` isn't a valid mode string
    pub fn new(modes: &str) -> Self {
        let mut channel = Channel {
            members: HashMap::new(),
            topic: None,
            user_limit: None,
            key: None,
            ban_mask: util::MaskSet::new(),
            exception_mask: util::MaskSet::new(),
            invex_mask: util::MaskSet::new(),
            invite_only: false,
            moderated: false,
            no_msg_from_outside: false,
            secret: false,
            topic_restricted: false,
        };
        for change in mode::simple_channel_query(modes).filter_map(Result::ok) {
            channel
                .apply_mode_change(change, usize::max_value(), |_| "")
                .unwrap();
        }
        channel
    }

    /// Adds a member with the default mode.
    pub fn add_member(&mut self, id: usize) {
        let modes = if self.members.is_empty() {
            MemberModes {
                founder: false,
                protected: false,
                operator: true,
                halfop: false,
                voice: false,
            }
        } else {
            MemberModes::default()
        };
        self.members.insert(id, modes);
    }

    pub fn list_entry(&self, msg: MessageBuffer<'_>) {
        msg.fmt_param(self.members.len()).trailing_param(
            self.topic
                .as_ref()
                .map_or("", |topic| topic.content.as_ref()),
        );
    }

    pub fn is_banned(&self, nick: &str) -> bool {
        self.ban_mask.is_match(nick)
            && !self.exception_mask.is_match(nick)
            && !self.invex_mask.is_match(nick)
    }

    pub fn is_invited(&self, nick: &str) -> bool {
        !self.invite_only || self.invex_mask.is_match(nick)
    }

    pub fn can_talk(&self, id: usize) -> bool {
        if let Some(member) = self.members.get(&id) {
            !self.moderated || member.has_voice()
        } else {
            !self.moderated && !self.no_msg_from_outside
        }
    }

    pub fn can_invite(&self, id: usize) -> bool {
        let member = match self.members.get(&id) {
            Some(member) => member,
            None => return false,
        };
        if self.invite_only {
            member.is_at_least_halfop()
        } else {
            true
        }
    }

    pub fn modes(&self, mut out: MessageBuffer<'_>, full_info: bool) {
        let modes = out.raw_param();
        modes.push('+');
        if self.invite_only {
            modes.push('i');
        }
        if self.moderated {
            modes.push('m');
        }
        if self.no_msg_from_outside {
            modes.push('n');
        }
        if self.secret {
            modes.push('s');
        }
        if self.topic_restricted {
            modes.push('t');
        }
        if self.user_limit.is_some() {
            modes.push('l');
        }
        if self.key.is_some() {
            modes.push('k');
        }

        if full_info {
            if let Some(user_limit) = self.user_limit {
                out = out.fmt_param(user_limit);
            }
            if let Some(ref key) = self.key {
                out.param(key);
            }
        }
    }

    pub fn apply_mode_change<'a>(
        &mut self,
        change: mode::ChannelChange<'_>,
        keylen: usize,
        nick_of: impl Fn(usize) -> &'a str,
    ) -> Result<bool, &'static str> {
        use mode::ChannelChange::*;

        let mut applied = false;
        match change {
            InviteOnly(value) => {
                applied = self.invite_only != value;
                self.invite_only = value;
            }
            Moderated(value) => {
                applied = self.moderated != value;
                self.moderated = value;
            }
            NoPrivMsgFromOutside(value) => {
                applied = self.no_msg_from_outside != value;
                self.no_msg_from_outside = value;
            }
            Secret(value) => {
                applied = self.secret != value;
                self.secret = value;
            }
            TopicRestricted(value) => {
                applied = self.topic_restricted != value;
                self.topic_restricted = value;
            }
            Key(value, key) => {
                if value {
                    if self.key.is_some() {
                        return Err(rpl::ERR_KEYSET);
                    } else {
                        applied = true;
                        self.key = Some(key[..key.len().min(keylen)].to_owned());
                    }
                } else if self.key.is_some() {
                    applied = true;
                    self.key = None;
                }
            }
            UserLimit(Some(s)) => {
                if let Ok(limit) = s.parse() {
                    applied = self
                        .user_limit
                        .map_or(true, |chan_limit| chan_limit != limit);
                    self.user_limit = Some(limit);
                }
            }
            UserLimit(None) => {
                applied = self.user_limit.is_some();
                self.user_limit = None;
            }
            ChangeBan(value, param) => {
                applied = if value {
                    self.ban_mask.insert(param)
                } else {
                    self.ban_mask.remove(param)
                };
            }
            ChangeException(value, param) => {
                applied = if value {
                    self.exception_mask.insert(param)
                } else {
                    self.exception_mask.remove(param)
                };
            }
            ChangeInvitation(value, param) => {
                applied = if value {
                    self.invex_mask.insert(param)
                } else {
                    self.invex_mask.remove(param)
                };
            }
            ChangeOperator(value, param) => {
                let mut has_it = false;
                for (member, modes) in &mut self.members {
                    if nick_of(*member) == param {
                        has_it = true;
                        applied = modes.operator != value;
                        modes.operator = value;
                        break;
                    }
                }
                if !has_it {
                    return Err(rpl::ERR_USERNOTINCHANNEL);
                }
            }
            ChangeHalfop(value, param) => {
                let mut has_it = false;
                for (member, modes) in &mut self.members {
                    if nick_of(*member) == param {
                        has_it = true;
                        applied = modes.halfop != value;
                        modes.halfop = value;
                        break;
                    }
                }
                if !has_it {
                    return Err(rpl::ERR_USERNOTINCHANNEL);
                }
            }
            ChangeVoice(value, param) => {
                let mut has_it = false;
                for (member, modes) in &mut self.members {
                    if nick_of(*member) == param {
                        has_it = true;
                        applied = modes.voice != value;
                        modes.voice = value;
                        break;
                    }
                }
                if !has_it {
                    return Err(rpl::ERR_USERNOTINCHANNEL);
                }
            }
            _ => {}
        }
        Ok(applied)
    }

    pub fn symbol(&self) -> &'static str {
        if self.secret {
            "@"
        } else {
            "="
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPERATOR: MemberModes = MemberModes {
        founder: false,
        protected: false,
        operator: true,
        halfop: true,
        voice: false,
    };
    const HALFOP: MemberModes = MemberModes {
        founder: false,
        protected: false,
        operator: false,
        halfop: true,
        voice: false,
    };
    const VOICE: MemberModes = MemberModes {
        founder: false,
        protected: false,
        operator: false,
        halfop: false,
        voice: true,
    };

    #[test]
    fn test_member_modes_is_at_least() {
        assert!(OPERATOR.has_voice());
        assert!(OPERATOR.is_at_least_halfop());
        assert!(OPERATOR.is_at_least_op());
        assert!(HALFOP.has_voice());
        assert!(HALFOP.is_at_least_halfop());
        assert!(!HALFOP.is_at_least_op());
        assert!(VOICE.has_voice());
        assert!(!VOICE.is_at_least_halfop());
        assert!(!VOICE.is_at_least_op());
    }
} // mod tests
