use crate::iq::node::{
    collect_children, optional_attr, optional_child, required_attr, required_child,
};
use crate::iq::spec::IqSpec;
use crate::protocol::ProtocolNode;
use crate::request::InfoQuery;
use crate::StringEnum;
use anyhow::{anyhow, Result};
use typed_builder::TypedBuilder;
use wacore_binary::builder::NodeBuilder;
use wacore_binary::jid::{Jid, GROUP_SERVER, SERVER_JID};
use wacore_binary::node::{Node, NodeContent};

// Re-export AddressingMode from types::message for convenience
pub use crate::types::message::AddressingMode;
/// IQ namespace for group operations.
pub const GROUP_IQ_NAMESPACE: &str = "w:g2";

/// Maximum length for a WhatsApp group subject (from `group_max_subject` A/B prop).
pub const GROUP_SUBJECT_MAX_LENGTH: usize = 100;

/// Maximum length for a WhatsApp group description (from `group_description_length` A/B prop).
pub const GROUP_DESCRIPTION_MAX_LENGTH: usize = 2048;

/// Maximum number of participants in a group (from `group_size_limit` A/B prop).
pub const GROUP_SIZE_LIMIT: usize = 257;
/// Member link mode for group invite links.
#[derive(Debug, Clone, Copy, PartialEq, Eq, StringEnum)]
pub enum MemberLinkMode {
    #[str = "admin_link"]
    AdminLink,
    #[str = "all_member_link"]
    AllMemberLink,
}

/// Member add mode for who can add participants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, StringEnum)]
pub enum MemberAddMode {
    #[str = "admin_add"]
    AdminAdd,
    #[str = "all_member_add"]
    AllMemberAdd,
}

/// Membership approval mode for join requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, StringEnum)]
pub enum MembershipApprovalMode {
    #[string_default]
    #[str = "off"]
    Off,
    #[str = "on"]
    On,
}

/// Query request type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, StringEnum)]
pub enum GroupQueryRequestType {
    #[string_default]
    #[str = "interactive"]
    Interactive,
}

/// Participant type (admin level).
#[derive(Debug, Clone, Copy, PartialEq, Eq, StringEnum)]
pub enum ParticipantType {
    #[string_default]
    #[str = "member"]
    Member,
    #[str = "admin"]
    Admin,
    #[str = "superadmin"]
    SuperAdmin,
}

impl ParticipantType {
    pub fn is_admin(&self) -> bool {
        matches!(self, ParticipantType::Admin | ParticipantType::SuperAdmin)
    }
}

impl TryFrom<Option<&str>> for ParticipantType {
    type Error = anyhow::Error;

    fn try_from(value: Option<&str>) -> Result<Self> {
        match value {
            Some("admin") => Ok(ParticipantType::Admin),
            Some("superadmin") => Ok(ParticipantType::SuperAdmin),
            Some("member") | None => Ok(ParticipantType::Member),
            Some(other) => Err(anyhow!("unknown participant type: {other}")),
        }
    }
}
crate::define_validated_string! {
    /// A validated group subject string.
    ///
    /// WhatsApp limits group subjects to [`GROUP_SUBJECT_MAX_LENGTH`] characters.
    pub struct GroupSubject(max_len = GROUP_SUBJECT_MAX_LENGTH, name = "Group subject")
}

crate::define_validated_string! {
    /// A validated group description string.
    ///
    /// WhatsApp limits group descriptions to [`GROUP_DESCRIPTION_MAX_LENGTH`] characters.
    pub struct GroupDescription(max_len = GROUP_DESCRIPTION_MAX_LENGTH, name = "Group description")
}
/// Options for a participant when creating a group.
#[derive(Debug, Clone, TypedBuilder)]
#[builder(build_method(into))]
pub struct GroupParticipantOptions {
    pub jid: Jid,
    #[builder(default, setter(strip_option))]
    pub phone_number: Option<Jid>,
    #[builder(default, setter(strip_option))]
    pub privacy: Option<Vec<u8>>,
}

impl GroupParticipantOptions {
    pub fn new(jid: Jid) -> Self {
        Self {
            jid,
            phone_number: None,
            privacy: None,
        }
    }

    pub fn from_phone(phone_number: Jid) -> Self {
        Self::new(phone_number)
    }

    pub fn from_lid_and_phone(lid: Jid, phone_number: Jid) -> Self {
        Self::new(lid).with_phone_number(phone_number)
    }

    pub fn with_phone_number(mut self, phone_number: Jid) -> Self {
        self.phone_number = Some(phone_number);
        self
    }

    pub fn with_privacy(mut self, privacy: Vec<u8>) -> Self {
        self.privacy = Some(privacy);
        self
    }
}

/// Options for creating a new group.
#[derive(Debug, Clone, TypedBuilder)]
#[builder(build_method(into))]
pub struct GroupCreateOptions {
    #[builder(setter(into))]
    pub subject: String,
    #[builder(default)]
    pub participants: Vec<GroupParticipantOptions>,
    #[builder(default = Some(MemberLinkMode::AdminLink), setter(strip_option))]
    pub member_link_mode: Option<MemberLinkMode>,
    #[builder(default = Some(MemberAddMode::AllMemberAdd), setter(strip_option))]
    pub member_add_mode: Option<MemberAddMode>,
    #[builder(default = Some(MembershipApprovalMode::Off), setter(strip_option))]
    pub membership_approval_mode: Option<MembershipApprovalMode>,
    #[builder(default = Some(0), setter(strip_option))]
    pub ephemeral_expiration: Option<u32>,
}

impl GroupCreateOptions {
    /// Create new options with just a subject (for backwards compatibility).
    pub fn new(subject: impl Into<String>) -> Self {
        Self {
            subject: subject.into(),
            ..Default::default()
        }
    }

    pub fn with_participant(mut self, participant: GroupParticipantOptions) -> Self {
        self.participants.push(participant);
        self
    }

    pub fn with_participants(mut self, participants: Vec<GroupParticipantOptions>) -> Self {
        self.participants = participants;
        self
    }

    pub fn with_member_link_mode(mut self, mode: MemberLinkMode) -> Self {
        self.member_link_mode = Some(mode);
        self
    }

    pub fn with_member_add_mode(mut self, mode: MemberAddMode) -> Self {
        self.member_add_mode = Some(mode);
        self
    }

    pub fn with_membership_approval_mode(mut self, mode: MembershipApprovalMode) -> Self {
        self.membership_approval_mode = Some(mode);
        self
    }

    pub fn with_ephemeral_expiration(mut self, expiration: u32) -> Self {
        self.ephemeral_expiration = Some(expiration);
        self
    }
}

impl Default for GroupCreateOptions {
    fn default() -> Self {
        Self {
            subject: String::new(),
            participants: Vec::new(),
            member_link_mode: Some(MemberLinkMode::AdminLink),
            member_add_mode: Some(MemberAddMode::AllMemberAdd),
            membership_approval_mode: Some(MembershipApprovalMode::Off),
            ephemeral_expiration: Some(0),
        }
    }
}

/// Normalize participants: drop phone_number for non-LID JIDs.
pub fn normalize_participants(
    participants: &[GroupParticipantOptions],
) -> Vec<GroupParticipantOptions> {
    participants
        .iter()
        .cloned()
        .map(|p| {
            if !p.jid.is_lid() && p.phone_number.is_some() {
                GroupParticipantOptions {
                    phone_number: None,
                    ..p
                }
            } else {
                p
            }
        })
        .collect()
}

/// Build the `<create>` node for group creation.
pub fn build_create_group_node(options: &GroupCreateOptions) -> Node {
    let mut children = Vec::new();

    if let Some(link_mode) = &options.member_link_mode {
        children.push(
            NodeBuilder::new("member_link_mode")
                .string_content(link_mode.as_str())
                .build(),
        );
    }

    if let Some(add_mode) = &options.member_add_mode {
        children.push(
            NodeBuilder::new("member_add_mode")
                .string_content(add_mode.as_str())
                .build(),
        );
    }

    // Normalize participants to avoid sending phone_number for non-LID JIDs
    let participants = normalize_participants(&options.participants);

    for participant in &participants {
        let mut attrs = vec![("jid", participant.jid.to_string())];
        if let Some(pn) = &participant.phone_number {
            attrs.push(("phone_number", pn.to_string()));
        }

        let participant_node = if let Some(privacy_bytes) = &participant.privacy {
            NodeBuilder::new("participant")
                .attrs(attrs)
                .children([NodeBuilder::new("privacy")
                    .string_content(hex::encode(privacy_bytes))
                    .build()])
                .build()
        } else {
            NodeBuilder::new("participant").attrs(attrs).build()
        };
        children.push(participant_node);
    }

    if let Some(expiration) = &options.ephemeral_expiration {
        children.push(
            NodeBuilder::new("ephemeral")
                .attr("expiration", expiration.to_string())
                .build(),
        );
    }

    if let Some(approval_mode) = &options.membership_approval_mode {
        children.push(
            NodeBuilder::new("membership_approval_mode")
                .children([NodeBuilder::new("group_join")
                    .attr("state", approval_mode.as_str())
                    .build()])
                .build(),
        );
    }

    NodeBuilder::new("create")
        .attr("subject", &options.subject)
        .children(children)
        .build()
}
/// Request to query group information.
#[derive(Debug, Clone, Default)]
pub struct GroupQueryRequest {
    pub request: GroupQueryRequestType,
}

impl ProtocolNode for GroupQueryRequest {
    fn tag(&self) -> &'static str {
        "query"
    }

    fn into_node(self) -> Node {
        NodeBuilder::new("query")
            .attr("request", self.request.as_str())
            .build()
    }

    fn try_from_node(node: &Node) -> Result<Self> {
        if node.tag != "query" {
            return Err(anyhow!("expected <query>, got <{}>", node.tag));
        }
        Ok(Self::default())
    }
}

/// A participant in a group response.
#[derive(Debug, Clone)]
pub struct GroupParticipantResponse {
    pub jid: Jid,
    pub phone_number: Option<Jid>,
    pub participant_type: ParticipantType,
}

impl ProtocolNode for GroupParticipantResponse {
    fn tag(&self) -> &'static str {
        "participant"
    }

    fn into_node(self) -> Node {
        let mut builder = NodeBuilder::new("participant").attr("jid", self.jid.to_string());
        if let Some(pn) = &self.phone_number {
            builder = builder.attr("phone_number", pn.to_string());
        }
        if self.participant_type != ParticipantType::Member {
            builder = builder.attr("type", self.participant_type.as_str());
        }
        builder.build()
    }

    fn try_from_node(node: &Node) -> Result<Self> {
        if node.tag != "participant" {
            return Err(anyhow!("expected <participant>, got <{}>", node.tag));
        }
        let jid = node
            .attrs()
            .optional_jid("jid")
            .ok_or_else(|| anyhow!("participant missing required 'jid' attribute"))?;
        let phone_number = node.attrs().optional_jid("phone_number");
        // Default to Member for unknown participant types to avoid failing the whole group parse
        let participant_type = ParticipantType::try_from(node.attrs().optional_string("type"))
            .unwrap_or(ParticipantType::Member);

        Ok(Self {
            jid,
            phone_number,
            participant_type,
        })
    }
}

/// Response from a group info query.
#[derive(Debug, Clone)]
pub struct GroupInfoResponse {
    pub id: Jid,
    pub subject: GroupSubject,
    pub addressing_mode: AddressingMode,
    pub participants: Vec<GroupParticipantResponse>,
}

impl ProtocolNode for GroupInfoResponse {
    fn tag(&self) -> &'static str {
        "group"
    }

    fn into_node(self) -> Node {
        let children: Vec<Node> = self
            .participants
            .into_iter()
            .map(|p| p.into_node())
            .collect();
        NodeBuilder::new("group")
            .attr("id", self.id.to_string())
            .attr("subject", self.subject.as_str())
            .attr("addressing_mode", self.addressing_mode.as_str())
            .children(children)
            .build()
    }

    fn try_from_node(node: &Node) -> Result<Self> {
        if node.tag != "group" {
            return Err(anyhow!("expected <group>, got <{}>", node.tag));
        }

        let id_str = required_attr(node, "id")?;
        let id = if id_str.contains('@') {
            id_str.parse()?
        } else {
            Jid::group(id_str)
        };

        let subject =
            GroupSubject::new_unchecked(optional_attr(node, "subject").unwrap_or_default());

        let addressing_mode =
            AddressingMode::try_from(optional_attr(node, "addressing_mode").unwrap_or("pn"))?;

        let participants = collect_children::<GroupParticipantResponse>(node, "participant")?;

        Ok(Self {
            id,
            subject,
            addressing_mode,
            participants,
        })
    }
}
/// Request to get all groups the user is participating in.
#[derive(Debug, Clone)]
pub struct GroupParticipatingRequest {
    pub include_participants: bool,
    pub include_description: bool,
}

impl GroupParticipatingRequest {
    pub fn new() -> Self {
        Self {
            include_participants: true,
            include_description: true,
        }
    }
}

impl Default for GroupParticipatingRequest {
    fn default() -> Self {
        Self::new()
    }
}

impl ProtocolNode for GroupParticipatingRequest {
    fn tag(&self) -> &'static str {
        "participating"
    }

    fn into_node(self) -> Node {
        let mut children = Vec::new();
        if self.include_participants {
            children.push(NodeBuilder::new("participants").build());
        }
        if self.include_description {
            children.push(NodeBuilder::new("description").build());
        }
        NodeBuilder::new("participating").children(children).build()
    }

    fn try_from_node(node: &Node) -> Result<Self> {
        if node.tag != "participating" {
            return Err(anyhow!("expected <participating>, got <{}>", node.tag));
        }
        Ok(Self::default())
    }
}

/// Response containing all groups the user is participating in.
#[derive(Debug, Clone, Default)]
pub struct GroupParticipatingResponse {
    pub groups: Vec<GroupInfoResponse>,
}

impl ProtocolNode for GroupParticipatingResponse {
    fn tag(&self) -> &'static str {
        "groups"
    }

    fn into_node(self) -> Node {
        let children: Vec<Node> = self.groups.into_iter().map(|g| g.into_node()).collect();
        NodeBuilder::new("groups").children(children).build()
    }

    fn try_from_node(node: &Node) -> Result<Self> {
        if node.tag != "groups" {
            return Err(anyhow!("expected <groups>, got <{}>", node.tag));
        }

        let groups = collect_children::<GroupInfoResponse>(node, "group")?;

        Ok(Self { groups })
    }
}
/// IQ specification for querying a specific group's info.
#[derive(Debug, Clone)]
pub struct GroupQueryIq {
    pub group_jid: Jid,
}

impl GroupQueryIq {
    pub fn new(group_jid: &Jid) -> Self {
        Self {
            group_jid: group_jid.clone(),
        }
    }
}

impl IqSpec for GroupQueryIq {
    type Response = GroupInfoResponse;

    fn build_iq(&self) -> InfoQuery<'static> {
        InfoQuery::get_ref(
            GROUP_IQ_NAMESPACE,
            &self.group_jid,
            Some(NodeContent::Nodes(vec![
                GroupQueryRequest::default().into_node()
            ])),
        )
    }

    fn parse_response(&self, response: &Node) -> Result<Self::Response> {
        let group_node = required_child(response, "group")?;
        GroupInfoResponse::try_from_node(group_node)
    }
}

/// IQ specification for getting all groups the user is participating in.
#[derive(Debug, Clone, Default)]
pub struct GroupParticipatingIq;

impl GroupParticipatingIq {
    pub fn new() -> Self {
        Self
    }
}

impl IqSpec for GroupParticipatingIq {
    type Response = GroupParticipatingResponse;

    fn build_iq(&self) -> InfoQuery<'static> {
        InfoQuery::get(
            GROUP_IQ_NAMESPACE,
            Jid::new("", GROUP_SERVER),
            Some(NodeContent::Nodes(vec![
                GroupParticipatingRequest::new().into_node()
            ])),
        )
    }

    fn parse_response(&self, response: &Node) -> Result<Self::Response> {
        let groups_node = required_child(response, "groups")?;
        GroupParticipatingResponse::try_from_node(groups_node)
    }
}

/// IQ specification for creating a new group.
#[derive(Debug, Clone)]
pub struct GroupCreateIq {
    pub options: GroupCreateOptions,
}

impl GroupCreateIq {
    pub fn new(options: GroupCreateOptions) -> Self {
        Self { options }
    }
}

impl IqSpec for GroupCreateIq {
    type Response = Jid;

    fn build_iq(&self) -> InfoQuery<'static> {
        InfoQuery::set(
            GROUP_IQ_NAMESPACE,
            Jid::new("", GROUP_SERVER),
            Some(NodeContent::Nodes(vec![build_create_group_node(
                &self.options,
            )])),
        )
    }

    fn parse_response(&self, response: &Node) -> Result<Self::Response> {
        let group_node = required_child(response, "group")?;
        let group_id_str = required_attr(group_node, "id")?;

        if group_id_str.contains('@') {
            group_id_str.parse().map_err(Into::into)
        } else {
            Ok(Jid::group(group_id_str))
        }
    }
}

// ---------------------------------------------------------------------------
// Group Management IQ Specs
// ---------------------------------------------------------------------------

/// Response for participant change operations (add/remove/promote/demote).
#[derive(Debug, Clone)]
pub struct ParticipantChangeResponse {
    pub jid: Jid,
    /// HTTP-like status code (e.g. 200, 403, 409).
    pub status: Option<String>,
    pub error: Option<String>,
}

impl ProtocolNode for ParticipantChangeResponse {
    fn tag(&self) -> &'static str {
        "participant"
    }

    fn into_node(self) -> Node {
        let mut builder = NodeBuilder::new("participant").attr("jid", self.jid.to_string());
        if let Some(ref status) = self.status {
            builder = builder.attr("type", status);
        }
        if let Some(ref error) = self.error {
            builder = builder.attr("error", error);
        }
        builder.build()
    }

    fn try_from_node(node: &Node) -> Result<Self> {
        if node.tag != "participant" {
            return Err(anyhow!("expected <participant>, got <{}>", node.tag));
        }
        let jid = node
            .attrs()
            .optional_jid("jid")
            .ok_or_else(|| anyhow!("participant missing required 'jid' attribute"))?;
        let status = optional_attr(node, "type").map(String::from);
        let error = optional_attr(node, "error").map(String::from);
        Ok(Self { jid, status, error })
    }
}

/// IQ specification for setting a group's subject.
///
/// Wire format:
/// ```xml
/// <iq type="set" xmlns="w:g2" to="{group_jid}">
///   <subject>{text}</subject>
/// </iq>
/// ```
#[derive(Debug, Clone)]
pub struct SetGroupSubjectIq {
    pub group_jid: Jid,
    pub subject: GroupSubject,
}

impl SetGroupSubjectIq {
    pub fn new(group_jid: &Jid, subject: GroupSubject) -> Self {
        Self {
            group_jid: group_jid.clone(),
            subject,
        }
    }
}

impl IqSpec for SetGroupSubjectIq {
    type Response = ();

    fn build_iq(&self) -> InfoQuery<'static> {
        InfoQuery::set_ref(
            GROUP_IQ_NAMESPACE,
            &self.group_jid,
            Some(NodeContent::Nodes(vec![NodeBuilder::new("subject")
                .string_content(self.subject.as_str())
                .build()])),
        )
    }

    fn parse_response(&self, _response: &Node) -> Result<Self::Response> {
        Ok(())
    }
}

/// IQ specification for setting a group's description.
///
/// Wire format:
/// ```xml
/// <iq type="set" xmlns="w:g2" to="{group_jid}">
///   <description id="{new_id}" prev="{prev_id}"><body>{text}</body></description>
/// </iq>
/// ```
///
/// - `id`: random 8-char hex, generated automatically.
/// - `prev`: the current description ID (from group metadata), used for conflict detection.
/// - To delete the description, pass `None` as the description.
#[derive(Debug, Clone)]
pub struct SetGroupDescriptionIq {
    pub group_jid: Jid,
    pub description: Option<GroupDescription>,
    /// New description ID (random 8-char hex).
    pub id: String,
    /// Previous description ID from group metadata, for conflict detection.
    pub prev: Option<String>,
}

impl SetGroupDescriptionIq {
    pub fn new(
        group_jid: &Jid,
        description: Option<GroupDescription>,
        prev: Option<String>,
    ) -> Self {
        use rand::Rng;
        let id = format!("{:08X}", rand::rng().random::<u32>());
        Self {
            group_jid: group_jid.clone(),
            description,
            id,
            prev,
        }
    }
}

impl IqSpec for SetGroupDescriptionIq {
    type Response = ();

    fn build_iq(&self) -> InfoQuery<'static> {
        let desc_node = if let Some(ref desc) = self.description {
            let mut builder = NodeBuilder::new("description").attr("id", &self.id);
            if let Some(ref prev) = self.prev {
                builder = builder.attr("prev", prev);
            }
            builder
                .children([NodeBuilder::new("body")
                    .string_content(desc.as_str())
                    .build()])
                .build()
        } else {
            let mut builder = NodeBuilder::new("description")
                .attr("id", &self.id)
                .attr("delete", "true");
            if let Some(ref prev) = self.prev {
                builder = builder.attr("prev", prev);
            }
            builder.build()
        };

        InfoQuery::set_ref(
            GROUP_IQ_NAMESPACE,
            &self.group_jid,
            Some(NodeContent::Nodes(vec![desc_node])),
        )
    }

    fn parse_response(&self, _response: &Node) -> Result<Self::Response> {
        Ok(())
    }
}

/// IQ specification for leaving a group.
///
/// Wire format:
/// ```xml
/// <iq type="set" xmlns="w:g2" to="g.us">
///   <leave><group id="{group_jid}"/></leave>
/// </iq>
/// ```
#[derive(Debug, Clone)]
pub struct LeaveGroupIq {
    pub group_jid: Jid,
}

impl LeaveGroupIq {
    pub fn new(group_jid: &Jid) -> Self {
        Self {
            group_jid: group_jid.clone(),
        }
    }
}

impl IqSpec for LeaveGroupIq {
    type Response = ();

    fn build_iq(&self) -> InfoQuery<'static> {
        let group_node = NodeBuilder::new("group")
            .attr("id", self.group_jid.to_string())
            .build();
        let leave_node = NodeBuilder::new("leave").children([group_node]).build();

        InfoQuery::set(
            GROUP_IQ_NAMESPACE,
            Jid::new("", GROUP_SERVER),
            Some(NodeContent::Nodes(vec![leave_node])),
        )
    }

    fn parse_response(&self, _response: &Node) -> Result<Self::Response> {
        Ok(())
    }
}

/// IQ specification for adding participants to a group.
///
/// Wire format:
/// ```xml
/// <iq type="set" xmlns="w:g2" to="{group_jid}">
///   <add>
///     <participant jid="{user_jid}"/>
///   </add>
/// </iq>
/// ```
#[derive(Debug, Clone)]
pub struct AddParticipantsIq {
    pub group_jid: Jid,
    pub participants: Vec<Jid>,
}

impl AddParticipantsIq {
    pub fn new(group_jid: &Jid, participants: &[Jid]) -> Self {
        Self {
            group_jid: group_jid.clone(),
            participants: participants.to_vec(),
        }
    }
}

impl IqSpec for AddParticipantsIq {
    type Response = Vec<ParticipantChangeResponse>;

    fn build_iq(&self) -> InfoQuery<'static> {
        let children: Vec<Node> = self
            .participants
            .iter()
            .map(|jid| {
                NodeBuilder::new("participant")
                    .attr("jid", jid.to_string())
                    .build()
            })
            .collect();

        let add_node = NodeBuilder::new("add").children(children).build();

        InfoQuery::set_ref(
            GROUP_IQ_NAMESPACE,
            &self.group_jid,
            Some(NodeContent::Nodes(vec![add_node])),
        )
    }

    fn parse_response(&self, response: &Node) -> Result<Self::Response> {
        let add_node = required_child(response, "add")?;
        collect_children::<ParticipantChangeResponse>(add_node, "participant")
    }
}

/// IQ specification for removing participants from a group.
///
/// Wire format:
/// ```xml
/// <iq type="set" xmlns="w:g2" to="{group_jid}">
///   <remove>
///     <participant jid="{user_jid}"/>
///   </remove>
/// </iq>
/// ```
#[derive(Debug, Clone)]
pub struct RemoveParticipantsIq {
    pub group_jid: Jid,
    pub participants: Vec<Jid>,
}

impl RemoveParticipantsIq {
    pub fn new(group_jid: &Jid, participants: &[Jid]) -> Self {
        Self {
            group_jid: group_jid.clone(),
            participants: participants.to_vec(),
        }
    }
}

impl IqSpec for RemoveParticipantsIq {
    type Response = Vec<ParticipantChangeResponse>;

    fn build_iq(&self) -> InfoQuery<'static> {
        let children: Vec<Node> = self
            .participants
            .iter()
            .map(|jid| {
                NodeBuilder::new("participant")
                    .attr("jid", jid.to_string())
                    .build()
            })
            .collect();

        let remove_node = NodeBuilder::new("remove").children(children).build();

        InfoQuery::set_ref(
            GROUP_IQ_NAMESPACE,
            &self.group_jid,
            Some(NodeContent::Nodes(vec![remove_node])),
        )
    }

    fn parse_response(&self, response: &Node) -> Result<Self::Response> {
        let remove_node = required_child(response, "remove")?;
        collect_children::<ParticipantChangeResponse>(remove_node, "participant")
    }
}

/// IQ specification for promoting participants to admin.
///
/// Wire format:
/// ```xml
/// <iq type="set" xmlns="w:g2" to="{group_jid}">
///   <promote>
///     <participant jid="{user_jid}"/>
///   </promote>
/// </iq>
/// ```
#[derive(Debug, Clone)]
pub struct PromoteParticipantsIq {
    pub group_jid: Jid,
    pub participants: Vec<Jid>,
}

impl PromoteParticipantsIq {
    pub fn new(group_jid: &Jid, participants: &[Jid]) -> Self {
        Self {
            group_jid: group_jid.clone(),
            participants: participants.to_vec(),
        }
    }
}

impl IqSpec for PromoteParticipantsIq {
    type Response = ();

    fn build_iq(&self) -> InfoQuery<'static> {
        let children: Vec<Node> = self
            .participants
            .iter()
            .map(|jid| {
                NodeBuilder::new("participant")
                    .attr("jid", jid.to_string())
                    .build()
            })
            .collect();

        let promote_node = NodeBuilder::new("promote").children(children).build();

        InfoQuery::set_ref(
            GROUP_IQ_NAMESPACE,
            &self.group_jid,
            Some(NodeContent::Nodes(vec![promote_node])),
        )
    }

    fn parse_response(&self, _response: &Node) -> Result<Self::Response> {
        Ok(())
    }
}

/// IQ specification for demoting participants from admin.
///
/// Wire format:
/// ```xml
/// <iq type="set" xmlns="w:g2" to="{group_jid}">
///   <demote>
///     <participant jid="{user_jid}"/>
///   </demote>
/// </iq>
/// ```
#[derive(Debug, Clone)]
pub struct DemoteParticipantsIq {
    pub group_jid: Jid,
    pub participants: Vec<Jid>,
}

impl DemoteParticipantsIq {
    pub fn new(group_jid: &Jid, participants: &[Jid]) -> Self {
        Self {
            group_jid: group_jid.clone(),
            participants: participants.to_vec(),
        }
    }
}

impl IqSpec for DemoteParticipantsIq {
    type Response = ();

    fn build_iq(&self) -> InfoQuery<'static> {
        let children: Vec<Node> = self
            .participants
            .iter()
            .map(|jid| {
                NodeBuilder::new("participant")
                    .attr("jid", jid.to_string())
                    .build()
            })
            .collect();

        let demote_node = NodeBuilder::new("demote").children(children).build();

        InfoQuery::set_ref(
            GROUP_IQ_NAMESPACE,
            &self.group_jid,
            Some(NodeContent::Nodes(vec![demote_node])),
        )
    }

    fn parse_response(&self, _response: &Node) -> Result<Self::Response> {
        Ok(())
    }
}

/// IQ specification for getting (or resetting) a group's invite link.
///
/// - `reset: false` (GET) fetches the existing link.
/// - `reset: true` (SET) revokes the old link and generates a new one.
///
/// Response: `<invite code="XXXX"/>`
#[derive(Debug, Clone)]
pub struct GetGroupInviteLinkIq {
    pub group_jid: Jid,
    pub reset: bool,
}

impl GetGroupInviteLinkIq {
    pub fn new(group_jid: &Jid, reset: bool) -> Self {
        Self {
            group_jid: group_jid.clone(),
            reset,
        }
    }
}

impl IqSpec for GetGroupInviteLinkIq {
    type Response = String;

    fn build_iq(&self) -> InfoQuery<'static> {
        let content = Some(NodeContent::Nodes(vec![NodeBuilder::new("invite").build()]));
        if self.reset {
            InfoQuery::set_ref(GROUP_IQ_NAMESPACE, &self.group_jid, content)
        } else {
            InfoQuery::get_ref(GROUP_IQ_NAMESPACE, &self.group_jid, content)
        }
    }

    fn parse_response(&self, response: &Node) -> Result<Self::Response> {
        let invite_node = required_child(response, "invite")?;
        let code = required_attr(invite_node, "code")?;
        Ok(format!("https://chat.whatsapp.com/{code}"))
    }
}

// ---------------------------------------------------------------------------
// Group Photo IQ Specs
// ---------------------------------------------------------------------------

/// IQ namespace for profile picture operations.
const PROFILE_PICTURE_NAMESPACE: &str = "w:profile:picture";

/// IQ specification for setting a group's profile photo.
///
/// Wire format (from `SetGroupPhoto` in Go):
/// ```xml
/// <iq xmlns="w:profile:picture" type="set" to="s.whatsapp.net" target="{group_jid}">
///   <picture type="image">PHOTO_BYTES</picture>
/// </iq>
/// ```
///
/// The photo should be JPEG. Returns the new picture ID on success.
#[derive(Debug, Clone)]
pub struct SetGroupPhotoIq {
    pub group_jid: Jid,
    pub photo: Vec<u8>,
}

impl SetGroupPhotoIq {
    pub fn new(group_jid: &Jid, photo: Vec<u8>) -> Self {
        Self {
            group_jid: group_jid.clone(),
            photo,
        }
    }
}

impl IqSpec for SetGroupPhotoIq {
    type Response = String;

    fn build_iq(&self) -> InfoQuery<'static> {
        let picture_node = NodeBuilder::new("picture")
            .attr("type", "image")
            .bytes(self.photo.clone())
            .build();

        InfoQuery::set(
            PROFILE_PICTURE_NAMESPACE,
            Jid::new("", SERVER_JID),
            Some(NodeContent::Nodes(vec![picture_node])),
        )
        .with_target(self.group_jid.clone())
    }

    fn parse_response(&self, response: &Node) -> Result<Self::Response> {
        let picture_node = required_child(response, "picture")?;
        let id = required_attr(picture_node, "id")?;
        Ok(id)
    }
}

/// IQ specification for deleting a group's profile photo.
///
/// Wire format (from `SetGroupPhoto` in Go with nil avatar):
/// ```xml
/// <iq xmlns="w:profile:picture" type="set" to="s.whatsapp.net" target="{group_jid}">
///   <picture type="image"/>
/// </iq>
/// ```
///
/// Sending an empty `<picture>` node deletes the current photo.
#[derive(Debug, Clone)]
pub struct DeleteGroupPhotoIq {
    pub group_jid: Jid,
}

impl DeleteGroupPhotoIq {
    pub fn new(group_jid: &Jid) -> Self {
        Self {
            group_jid: group_jid.clone(),
        }
    }
}

impl IqSpec for DeleteGroupPhotoIq {
    type Response = ();

    fn build_iq(&self) -> InfoQuery<'static> {
        let picture_node = NodeBuilder::new("picture").attr("type", "image").build();

        InfoQuery::set(
            PROFILE_PICTURE_NAMESPACE,
            Jid::new("", SERVER_JID),
            Some(NodeContent::Nodes(vec![picture_node])),
        )
        .with_target(self.group_jid.clone())
    }

    fn parse_response(&self, _response: &Node) -> Result<Self::Response> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Invite Link IQ Specs
// ---------------------------------------------------------------------------

/// IQ specification for joining a group via an invite link.
///
/// Wire format (from `JoinGroupWithLink` in Go):
/// ```xml
/// <iq type="set" xmlns="w:g2" to="g.us">
///   <invite code="INVITE_CODE"/>
/// </iq>
/// ```
///
/// Returns the group JID on success. If the group requires approval, the
/// response contains `<membership_approval_request jid="..."/>` instead.
#[derive(Debug, Clone)]
pub struct JoinGroupWithLinkIq {
    pub invite_code: String,
}

impl JoinGroupWithLinkIq {
    pub fn new(invite_code: impl Into<String>) -> Self {
        let code = invite_code.into();
        // Strip the URL prefix if provided
        let code = code
            .strip_prefix("https://chat.whatsapp.com/")
            .map(String::from)
            .unwrap_or(code);
        Self { invite_code: code }
    }
}

/// Response from joining a group via invite link.
#[derive(Debug, Clone)]
pub enum JoinGroupWithLinkResponse {
    /// Successfully joined; contains the group JID.
    Joined(Jid),
    /// Group requires approval; contains the group JID from the approval request.
    PendingApproval(Jid),
}

impl IqSpec for JoinGroupWithLinkIq {
    type Response = JoinGroupWithLinkResponse;

    fn build_iq(&self) -> InfoQuery<'static> {
        let invite_node = NodeBuilder::new("invite")
            .attr("code", &self.invite_code)
            .build();

        InfoQuery::set(
            GROUP_IQ_NAMESPACE,
            Jid::new("", GROUP_SERVER),
            Some(NodeContent::Nodes(vec![invite_node])),
        )
    }

    fn parse_response(&self, response: &Node) -> Result<Self::Response> {
        // Check for membership approval request first
        if let Some(approval_node) = optional_child(response, "membership_approval_request") {
            let jid = approval_node
                .attrs()
                .optional_jid("jid")
                .ok_or_else(|| anyhow!("membership_approval_request missing 'jid'"))?;
            return Ok(JoinGroupWithLinkResponse::PendingApproval(jid));
        }

        let group_node = required_child(response, "group")?;
        let jid = group_node
            .attrs()
            .optional_jid("jid")
            .ok_or_else(|| anyhow!("group response missing 'jid'"))?;
        Ok(JoinGroupWithLinkResponse::Joined(jid))
    }
}

/// IQ specification for getting group info from an invite link (without joining).
///
/// Wire format (from `GetGroupInfoFromLink` in Go):
/// ```xml
/// <iq type="get" xmlns="w:g2" to="g.us">
///   <invite code="INVITE_CODE"/>
/// </iq>
/// ```
#[derive(Debug, Clone)]
pub struct GetGroupInfoFromLinkIq {
    pub invite_code: String,
}

impl GetGroupInfoFromLinkIq {
    pub fn new(invite_code: impl Into<String>) -> Self {
        let code = invite_code.into();
        let code = code
            .strip_prefix("https://chat.whatsapp.com/")
            .map(String::from)
            .unwrap_or(code);
        Self { invite_code: code }
    }
}

impl IqSpec for GetGroupInfoFromLinkIq {
    type Response = GroupInfoResponse;

    fn build_iq(&self) -> InfoQuery<'static> {
        let invite_node = NodeBuilder::new("invite")
            .attr("code", &self.invite_code)
            .build();

        InfoQuery::get(
            GROUP_IQ_NAMESPACE,
            Jid::new("", GROUP_SERVER),
            Some(NodeContent::Nodes(vec![invite_node])),
        )
    }

    fn parse_response(&self, response: &Node) -> Result<Self::Response> {
        let group_node = required_child(response, "group")?;
        GroupInfoResponse::try_from_node(group_node)
    }
}

/// IQ specification for joining a group using an invite message (not a link).
///
/// Wire format (from `JoinGroupWithInvite` in Go):
/// ```xml
/// <iq type="set" xmlns="w:g2" to="{group_jid}">
///   <accept code="CODE" expiration="TIMESTAMP" admin="INVITER_JID"/>
/// </iq>
/// ```
#[derive(Debug, Clone)]
pub struct JoinGroupWithInviteIq {
    pub group_jid: Jid,
    pub inviter: Jid,
    pub code: String,
    pub expiration: i64,
}

impl JoinGroupWithInviteIq {
    pub fn new(group_jid: &Jid, inviter: &Jid, code: impl Into<String>, expiration: i64) -> Self {
        Self {
            group_jid: group_jid.clone(),
            inviter: inviter.clone(),
            code: code.into(),
            expiration,
        }
    }
}

impl IqSpec for JoinGroupWithInviteIq {
    type Response = ();

    fn build_iq(&self) -> InfoQuery<'static> {
        let accept_node = NodeBuilder::new("accept")
            .attr("code", &self.code)
            .attr("expiration", self.expiration.to_string())
            .attr("admin", self.inviter.to_string())
            .build();

        InfoQuery::set_ref(
            GROUP_IQ_NAMESPACE,
            &self.group_jid,
            Some(NodeContent::Nodes(vec![accept_node])),
        )
    }

    fn parse_response(&self, _response: &Node) -> Result<Self::Response> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Group Settings IQ Specs
// ---------------------------------------------------------------------------

/// IQ specification for setting group locked status (only admins can edit group info).
///
/// Wire format (from `SetGroupLocked` in Go):
/// ```xml
/// <iq type="set" xmlns="w:g2" to="{group_jid}">
///   <locked/>       <!-- or -->
///   <unlocked/>
/// </iq>
/// ```
#[derive(Debug, Clone)]
pub struct SetGroupLockedIq {
    pub group_jid: Jid,
    pub locked: bool,
}

impl SetGroupLockedIq {
    pub fn new(group_jid: &Jid, locked: bool) -> Self {
        Self {
            group_jid: group_jid.clone(),
            locked,
        }
    }
}

impl IqSpec for SetGroupLockedIq {
    type Response = ();

    fn build_iq(&self) -> InfoQuery<'static> {
        let tag = if self.locked { "locked" } else { "unlocked" };
        let node = NodeBuilder::new(tag).build();

        InfoQuery::set_ref(
            GROUP_IQ_NAMESPACE,
            &self.group_jid,
            Some(NodeContent::Nodes(vec![node])),
        )
    }

    fn parse_response(&self, _response: &Node) -> Result<Self::Response> {
        Ok(())
    }
}

/// IQ specification for setting group announce mode (only admins can send messages).
///
/// Wire format (from `SetGroupAnnounce` in Go):
/// ```xml
/// <iq type="set" xmlns="w:g2" to="{group_jid}">
///   <announcement/>       <!-- or -->
///   <not_announcement/>
/// </iq>
/// ```
#[derive(Debug, Clone)]
pub struct SetGroupAnnounceIq {
    pub group_jid: Jid,
    pub announce: bool,
}

impl SetGroupAnnounceIq {
    pub fn new(group_jid: &Jid, announce: bool) -> Self {
        Self {
            group_jid: group_jid.clone(),
            announce,
        }
    }
}

impl IqSpec for SetGroupAnnounceIq {
    type Response = ();

    fn build_iq(&self) -> InfoQuery<'static> {
        let tag = if self.announce {
            "announcement"
        } else {
            "not_announcement"
        };
        let node = NodeBuilder::new(tag).build();

        InfoQuery::set_ref(
            GROUP_IQ_NAMESPACE,
            &self.group_jid,
            Some(NodeContent::Nodes(vec![node])),
        )
    }

    fn parse_response(&self, _response: &Node) -> Result<Self::Response> {
        Ok(())
    }
}

/// IQ specification for setting group join approval mode (membership approval).
///
/// Wire format (from `SetGroupJoinApprovalMode` in Go):
/// ```xml
/// <iq type="set" xmlns="w:g2" to="{group_jid}">
///   <membership_approval_mode>
///     <group_join state="on"/>   <!-- or state="off" -->
///   </membership_approval_mode>
/// </iq>
/// ```
#[derive(Debug, Clone)]
pub struct SetGroupJoinApprovalIq {
    pub group_jid: Jid,
    pub enabled: bool,
}

impl SetGroupJoinApprovalIq {
    pub fn new(group_jid: &Jid, enabled: bool) -> Self {
        Self {
            group_jid: group_jid.clone(),
            enabled,
        }
    }
}

impl IqSpec for SetGroupJoinApprovalIq {
    type Response = ();

    fn build_iq(&self) -> InfoQuery<'static> {
        let state = if self.enabled { "on" } else { "off" };
        let group_join_node = NodeBuilder::new("group_join").attr("state", state).build();
        let approval_node = NodeBuilder::new("membership_approval_mode")
            .children([group_join_node])
            .build();

        InfoQuery::set_ref(
            GROUP_IQ_NAMESPACE,
            &self.group_jid,
            Some(NodeContent::Nodes(vec![approval_node])),
        )
    }

    fn parse_response(&self, _response: &Node) -> Result<Self::Response> {
        Ok(())
    }
}

/// IQ specification for setting who can add group members.
///
/// Wire format (from `SetGroupMemberAddMode` in Go):
/// ```xml
/// <iq type="set" xmlns="w:g2" to="{group_jid}">
///   <member_add_mode>admin_add</member_add_mode>   <!-- or all_member_add -->
/// </iq>
/// ```
#[derive(Debug, Clone)]
pub struct SetGroupMemberAddModeIq {
    pub group_jid: Jid,
    pub mode: MemberAddMode,
}

impl SetGroupMemberAddModeIq {
    pub fn new(group_jid: &Jid, mode: MemberAddMode) -> Self {
        Self {
            group_jid: group_jid.clone(),
            mode,
        }
    }
}

impl IqSpec for SetGroupMemberAddModeIq {
    type Response = ();

    fn build_iq(&self) -> InfoQuery<'static> {
        let node = NodeBuilder::new("member_add_mode")
            .string_content(self.mode.as_str())
            .build();

        InfoQuery::set_ref(
            GROUP_IQ_NAMESPACE,
            &self.group_jid,
            Some(NodeContent::Nodes(vec![node])),
        )
    }

    fn parse_response(&self, _response: &Node) -> Result<Self::Response> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Community / Linked Group IQ Specs
// ---------------------------------------------------------------------------

/// Link type for community group operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, StringEnum)]
pub enum GroupLinkType {
    #[str = "parent_group"]
    Parent,
    #[str = "sub_group"]
    Sub,
    #[str = "sibling_group"]
    Sibling,
}

/// Unlink reason for community group operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, StringEnum)]
pub enum GroupUnlinkReason {
    #[string_default]
    #[str = "unlink_group"]
    Default,
    #[str = "delete_parent"]
    DeleteParent,
}

/// IQ specification for linking a group as a sub-group of a community.
///
/// Wire format (from `LinkGroup` in Go):
/// ```xml
/// <iq type="set" xmlns="w:g2" to="{parent_jid}">
///   <links>
///     <link link_type="sub_group">
///       <group jid="{child_jid}"/>
///     </link>
///   </links>
/// </iq>
/// ```
#[derive(Debug, Clone)]
pub struct LinkGroupIq {
    pub parent_jid: Jid,
    pub child_jid: Jid,
}

impl LinkGroupIq {
    pub fn new(parent_jid: &Jid, child_jid: &Jid) -> Self {
        Self {
            parent_jid: parent_jid.clone(),
            child_jid: child_jid.clone(),
        }
    }
}

impl IqSpec for LinkGroupIq {
    type Response = ();

    fn build_iq(&self) -> InfoQuery<'static> {
        let group_node = NodeBuilder::new("group")
            .attr("jid", self.child_jid.to_string())
            .build();
        let link_node = NodeBuilder::new("link")
            .attr("link_type", GroupLinkType::Sub.as_str())
            .children([group_node])
            .build();
        let links_node = NodeBuilder::new("links").children([link_node]).build();

        InfoQuery::set_ref(
            GROUP_IQ_NAMESPACE,
            &self.parent_jid,
            Some(NodeContent::Nodes(vec![links_node])),
        )
    }

    fn parse_response(&self, _response: &Node) -> Result<Self::Response> {
        Ok(())
    }
}

/// IQ specification for unlinking a sub-group from a community.
///
/// Wire format (from `UnlinkGroup` in Go):
/// ```xml
/// <iq type="set" xmlns="w:g2" to="{parent_jid}">
///   <unlink unlink_type="sub_group">
///     <group jid="{child_jid}"/>
///   </unlink>
/// </iq>
/// ```
#[derive(Debug, Clone)]
pub struct UnlinkGroupIq {
    pub parent_jid: Jid,
    pub child_jid: Jid,
}

impl UnlinkGroupIq {
    pub fn new(parent_jid: &Jid, child_jid: &Jid) -> Self {
        Self {
            parent_jid: parent_jid.clone(),
            child_jid: child_jid.clone(),
        }
    }
}

impl IqSpec for UnlinkGroupIq {
    type Response = ();

    fn build_iq(&self) -> InfoQuery<'static> {
        let group_node = NodeBuilder::new("group")
            .attr("jid", self.child_jid.to_string())
            .build();
        let unlink_node = NodeBuilder::new("unlink")
            .attr("unlink_type", GroupLinkType::Sub.as_str())
            .children([group_node])
            .build();

        InfoQuery::set_ref(
            GROUP_IQ_NAMESPACE,
            &self.parent_jid,
            Some(NodeContent::Nodes(vec![unlink_node])),
        )
    }

    fn parse_response(&self, _response: &Node) -> Result<Self::Response> {
        Ok(())
    }
}

/// A sub-group target within a community.
#[derive(Debug, Clone)]
pub struct GroupLinkTarget {
    pub jid: Jid,
    pub subject: Option<String>,
    pub subject_time: Option<String>,
    pub is_default_sub_group: bool,
}

impl ProtocolNode for GroupLinkTarget {
    fn tag(&self) -> &'static str {
        "group"
    }

    fn into_node(self) -> Node {
        let mut builder = NodeBuilder::new("group").attr("jid", self.jid.to_string());
        if let Some(ref subject) = self.subject {
            builder = builder.attr("subject", subject);
        }
        if let Some(ref s_t) = self.subject_time {
            builder = builder.attr("s_t", s_t);
        }
        if self.is_default_sub_group {
            builder = builder.children([NodeBuilder::new("default_sub_group").build()]);
        }
        builder.build()
    }

    fn try_from_node(node: &Node) -> Result<Self> {
        if node.tag != "group" {
            return Err(anyhow!("expected <group>, got <{}>", node.tag));
        }
        // Accept either "jid" attr or construct from "id" attr
        let jid = if let Some(jid) = node.attrs().optional_jid("jid") {
            jid
        } else {
            let id = required_attr(node, "id")?;
            Jid::group(id)
        };
        let subject = optional_attr(node, "subject").map(String::from);
        let subject_time = optional_attr(node, "s_t").map(String::from);
        let is_default_sub_group = node.get_optional_child("default_sub_group").is_some();
        Ok(Self {
            jid,
            subject,
            subject_time,
            is_default_sub_group,
        })
    }
}

/// IQ specification for getting sub-groups of a community.
///
/// Wire format (from `GetSubGroups` in Go):
/// ```xml
/// <iq type="get" xmlns="w:g2" to="{community_jid}">
///   <sub_groups/>
/// </iq>
/// ```
///
/// Response: `<sub_groups><group jid="..." subject="..."/>...</sub_groups>`
#[derive(Debug, Clone)]
pub struct GetSubGroupsIq {
    pub community_jid: Jid,
}

impl GetSubGroupsIq {
    pub fn new(community_jid: &Jid) -> Self {
        Self {
            community_jid: community_jid.clone(),
        }
    }
}

impl IqSpec for GetSubGroupsIq {
    type Response = Vec<GroupLinkTarget>;

    fn build_iq(&self) -> InfoQuery<'static> {
        let node = NodeBuilder::new("sub_groups").build();

        InfoQuery::get_ref(
            GROUP_IQ_NAMESPACE,
            &self.community_jid,
            Some(NodeContent::Nodes(vec![node])),
        )
    }

    fn parse_response(&self, response: &Node) -> Result<Self::Response> {
        let sub_groups_node = required_child(response, "sub_groups")?;
        collect_children::<GroupLinkTarget>(sub_groups_node, "group")
    }
}

/// IQ specification for getting all participants across a community's linked groups.
///
/// Wire format (from `GetLinkedGroupsParticipants` in Go):
/// ```xml
/// <iq type="get" xmlns="w:g2" to="{community_jid}">
///   <linked_groups_participants/>
/// </iq>
/// ```
///
/// Response: `<linked_groups_participants><participant jid="..."/>...</linked_groups_participants>`
#[derive(Debug, Clone)]
pub struct GetLinkedGroupsParticipantsIq {
    pub community_jid: Jid,
}

impl GetLinkedGroupsParticipantsIq {
    pub fn new(community_jid: &Jid) -> Self {
        Self {
            community_jid: community_jid.clone(),
        }
    }
}

impl IqSpec for GetLinkedGroupsParticipantsIq {
    type Response = Vec<Jid>;

    fn build_iq(&self) -> InfoQuery<'static> {
        let node = NodeBuilder::new("linked_groups_participants").build();

        InfoQuery::get_ref(
            GROUP_IQ_NAMESPACE,
            &self.community_jid,
            Some(NodeContent::Nodes(vec![node])),
        )
    }

    fn parse_response(&self, response: &Node) -> Result<Self::Response> {
        let container = required_child(response, "linked_groups_participants")?;
        let mut participants = Vec::new();
        for child in container.get_children_by_tag("participant") {
            if let Some(jid) = child.attrs().optional_jid("jid") {
                participants.push(jid);
            }
        }
        Ok(participants)
    }
}

// ---------------------------------------------------------------------------
// Group Request Participants IQ Specs
// ---------------------------------------------------------------------------

/// A pending join request for a group.
#[derive(Debug, Clone)]
pub struct GroupParticipantRequest {
    pub jid: Jid,
    pub request_time: Option<String>,
}

/// IQ specification for getting pending join requests for a group.
///
/// Wire format (from `GetGroupRequestParticipants` in Go):
/// ```xml
/// <iq type="get" xmlns="w:g2" to="{group_jid}">
///   <membership_approval_requests/>
/// </iq>
/// ```
///
/// Response:
/// ```xml
/// <membership_approval_requests>
///   <membership_approval_request jid="..." request_time="..."/>
/// </membership_approval_requests>
/// ```
#[derive(Debug, Clone)]
pub struct GetGroupRequestParticipantsIq {
    pub group_jid: Jid,
}

impl GetGroupRequestParticipantsIq {
    pub fn new(group_jid: &Jid) -> Self {
        Self {
            group_jid: group_jid.clone(),
        }
    }
}

impl IqSpec for GetGroupRequestParticipantsIq {
    type Response = Vec<GroupParticipantRequest>;

    fn build_iq(&self) -> InfoQuery<'static> {
        let node = NodeBuilder::new("membership_approval_requests").build();

        InfoQuery::get_ref(
            GROUP_IQ_NAMESPACE,
            &self.group_jid,
            Some(NodeContent::Nodes(vec![node])),
        )
    }

    fn parse_response(&self, response: &Node) -> Result<Self::Response> {
        let container = required_child(response, "membership_approval_requests")?;
        let mut requests = Vec::new();
        for child in container.get_children_by_tag("membership_approval_request") {
            let jid = child
                .attrs()
                .optional_jid("jid")
                .ok_or_else(|| anyhow!("membership_approval_request missing 'jid'"))?;
            let request_time = optional_attr(child, "request_time").map(String::from);
            requests.push(GroupParticipantRequest { jid, request_time });
        }
        Ok(requests)
    }
}

/// Action for updating group join request participants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, StringEnum)]
pub enum ParticipantRequestAction {
    #[str = "approve"]
    Approve,
    #[str = "reject"]
    Reject,
}

/// IQ specification for approving or rejecting group join requests.
///
/// Wire format (from `UpdateGroupRequestParticipants` in Go):
/// ```xml
/// <iq type="set" xmlns="w:g2" to="{group_jid}">
///   <membership_requests_action>
///     <approve>                     <!-- or <reject> -->
///       <participant jid="..."/>
///     </approve>
///   </membership_requests_action>
/// </iq>
/// ```
#[derive(Debug, Clone)]
pub struct UpdateGroupRequestParticipantsIq {
    pub group_jid: Jid,
    pub participants: Vec<Jid>,
    pub action: ParticipantRequestAction,
}

impl UpdateGroupRequestParticipantsIq {
    pub fn new(group_jid: &Jid, participants: &[Jid], action: ParticipantRequestAction) -> Self {
        Self {
            group_jid: group_jid.clone(),
            participants: participants.to_vec(),
            action,
        }
    }
}

impl IqSpec for UpdateGroupRequestParticipantsIq {
    type Response = Vec<ParticipantChangeResponse>;

    fn build_iq(&self) -> InfoQuery<'static> {
        let participant_nodes: Vec<Node> = self
            .participants
            .iter()
            .map(|jid| {
                NodeBuilder::new("participant")
                    .attr("jid", jid.to_string())
                    .build()
            })
            .collect();

        let action_node = NodeBuilder::new(self.action.as_str())
            .children(participant_nodes)
            .build();
        let wrapper_node = NodeBuilder::new("membership_requests_action")
            .children([action_node])
            .build();

        InfoQuery::set_ref(
            GROUP_IQ_NAMESPACE,
            &self.group_jid,
            Some(NodeContent::Nodes(vec![wrapper_node])),
        )
    }

    fn parse_response(&self, response: &Node) -> Result<Self::Response> {
        let wrapper = required_child(response, "membership_requests_action")?;
        let action_node = required_child(wrapper, self.action.as_str())?;
        collect_children::<ParticipantChangeResponse>(action_node, "participant")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::InfoQueryType;

    #[test]
    fn test_group_subject_validation() {
        let subject = GroupSubject::new("Test Group").unwrap();
        assert_eq!(subject.as_str(), "Test Group");

        let at_limit = "a".repeat(GROUP_SUBJECT_MAX_LENGTH);
        assert!(GroupSubject::new(&at_limit).is_ok());

        let over_limit = "a".repeat(GROUP_SUBJECT_MAX_LENGTH + 1);
        assert!(GroupSubject::new(&over_limit).is_err());
    }

    #[test]
    fn test_group_description_validation() {
        let desc = GroupDescription::new("Test Description").unwrap();
        assert_eq!(desc.as_str(), "Test Description");

        let at_limit = "a".repeat(GROUP_DESCRIPTION_MAX_LENGTH);
        assert!(GroupDescription::new(&at_limit).is_ok());

        let over_limit = "a".repeat(GROUP_DESCRIPTION_MAX_LENGTH + 1);
        assert!(GroupDescription::new(&over_limit).is_err());
    }

    #[test]
    fn test_string_enum_member_add_mode() {
        assert_eq!(MemberAddMode::AdminAdd.as_str(), "admin_add");
        assert_eq!(MemberAddMode::AllMemberAdd.as_str(), "all_member_add");
        assert_eq!(
            MemberAddMode::try_from("admin_add").unwrap(),
            MemberAddMode::AdminAdd
        );
        assert!(MemberAddMode::try_from("invalid").is_err());
    }

    #[test]
    fn test_string_enum_member_link_mode() {
        assert_eq!(MemberLinkMode::AdminLink.as_str(), "admin_link");
        assert_eq!(MemberLinkMode::AllMemberLink.as_str(), "all_member_link");
        assert_eq!(
            MemberLinkMode::try_from("admin_link").unwrap(),
            MemberLinkMode::AdminLink
        );
    }

    #[test]
    fn test_participant_type_is_admin() {
        assert!(!ParticipantType::Member.is_admin());
        assert!(ParticipantType::Admin.is_admin());
        assert!(ParticipantType::SuperAdmin.is_admin());
    }

    #[test]
    fn test_normalize_participants_drops_phone_for_pn() {
        let pn_jid: Jid = "15551234567@s.whatsapp.net".parse().unwrap();
        let lid_jid: Jid = "100000000000001@lid".parse().unwrap();
        let phone_jid: Jid = "15550000001@s.whatsapp.net".parse().unwrap();

        let participants = vec![
            GroupParticipantOptions::new(pn_jid.clone()).with_phone_number(phone_jid.clone()),
            GroupParticipantOptions::new(lid_jid.clone()).with_phone_number(phone_jid.clone()),
        ];

        let normalized = normalize_participants(&participants);
        assert!(normalized[0].phone_number.is_none());
        assert_eq!(normalized[0].jid, pn_jid);
        assert_eq!(normalized[1].phone_number.as_ref(), Some(&phone_jid));
    }

    #[test]
    fn test_build_create_group_node() {
        let pn_jid: Jid = "15551234567@s.whatsapp.net".parse().unwrap();
        let options = GroupCreateOptions::new("Test Subject")
            .with_participant(GroupParticipantOptions::from_phone(pn_jid))
            .with_member_link_mode(MemberLinkMode::AllMemberLink)
            .with_member_add_mode(MemberAddMode::AdminAdd);

        let node = build_create_group_node(&options);
        assert_eq!(node.tag, "create");
        assert_eq!(
            node.attrs().optional_string("subject"),
            Some("Test Subject")
        );

        let link_mode = node.get_children_by_tag("member_link_mode").next().unwrap();
        assert_eq!(
            link_mode.content.as_ref().and_then(|c| match c {
                NodeContent::String(s) => Some(s.as_str()),
                _ => None,
            }),
            Some("all_member_link")
        );
    }

    #[test]
    fn test_typed_builder() {
        let options: GroupCreateOptions = GroupCreateOptions::builder()
            .subject("My Group")
            .member_add_mode(MemberAddMode::AdminAdd)
            .build();

        assert_eq!(options.subject, "My Group");
        assert_eq!(options.member_add_mode, Some(MemberAddMode::AdminAdd));
    }

    #[test]
    fn test_set_group_description_with_id_and_prev() {
        let jid: Jid = "120363000000000001@g.us".parse().unwrap();
        let desc = GroupDescription::new("New description").unwrap();
        let spec = SetGroupDescriptionIq::new(&jid, Some(desc), Some("AABBCCDD".to_string()));
        let iq = spec.build_iq();

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            let desc_node = &nodes[0];
            assert_eq!(desc_node.tag, "description");
            // id is random hex, just check it exists and is 8 chars
            let id = desc_node.attrs().optional_string("id").unwrap();
            assert_eq!(id.len(), 8);
            assert_eq!(desc_node.attrs().optional_string("prev"), Some("AABBCCDD"));
            // Should have a <body> child
            assert!(desc_node.get_children_by_tag("body").next().is_some());
        } else {
            panic!("expected nodes content");
        }
    }

    #[test]
    fn test_set_group_description_delete() {
        let jid: Jid = "120363000000000001@g.us".parse().unwrap();
        let spec = SetGroupDescriptionIq::new(&jid, None, Some("PREV1234".to_string()));
        let iq = spec.build_iq();

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            let desc_node = &nodes[0];
            assert_eq!(desc_node.tag, "description");
            assert_eq!(desc_node.attrs().optional_string("delete"), Some("true"));
            assert_eq!(desc_node.attrs().optional_string("prev"), Some("PREV1234"));
            // id should still be present
            assert!(desc_node.attrs().optional_string("id").is_some());
        } else {
            panic!("expected nodes content");
        }
    }

    #[test]
    fn test_leave_group_iq() {
        let jid: Jid = "120363000000000001@g.us".parse().unwrap();
        let spec = LeaveGroupIq::new(&jid);
        let iq = spec.build_iq();

        assert_eq!(iq.namespace, GROUP_IQ_NAMESPACE);
        assert_eq!(iq.query_type, InfoQueryType::Set);
        // Leave goes to g.us, not the group JID
        assert_eq!(iq.to.server, GROUP_SERVER);
    }

    #[test]
    fn test_add_participants_iq() {
        let group: Jid = "120363000000000001@g.us".parse().unwrap();
        let p1: Jid = "1234567890@s.whatsapp.net".parse().unwrap();
        let p2: Jid = "9876543210@s.whatsapp.net".parse().unwrap();
        let spec = AddParticipantsIq::new(&group, &[p1, p2]);
        let iq = spec.build_iq();

        assert_eq!(iq.namespace, GROUP_IQ_NAMESPACE);
        assert_eq!(iq.to, group);

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            let add_node = &nodes[0];
            assert_eq!(add_node.tag, "add");
            let participants: Vec<_> = add_node.get_children_by_tag("participant").collect();
            assert_eq!(participants.len(), 2);
        } else {
            panic!("expected nodes content");
        }
    }

    #[test]
    fn test_promote_demote_iq() {
        let group: Jid = "120363000000000001@g.us".parse().unwrap();
        let p1: Jid = "1234567890@s.whatsapp.net".parse().unwrap();

        let promote = PromoteParticipantsIq::new(&group, std::slice::from_ref(&p1));
        let iq = promote.build_iq();
        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            assert_eq!(nodes[0].tag, "promote");
        } else {
            panic!("expected nodes content");
        }

        let demote = DemoteParticipantsIq::new(&group, &[p1]);
        let iq = demote.build_iq();
        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            assert_eq!(nodes[0].tag, "demote");
        } else {
            panic!("expected nodes content");
        }
    }

    #[test]
    fn test_get_group_invite_link_iq() {
        let jid: Jid = "120363000000000001@g.us".parse().unwrap();
        let spec = GetGroupInviteLinkIq::new(&jid, false);
        let iq = spec.build_iq();

        assert_eq!(iq.query_type, InfoQueryType::Get);
        assert_eq!(iq.to, jid);

        // With reset=true it should be a SET
        let reset_spec = GetGroupInviteLinkIq::new(&jid, true);
        assert_eq!(reset_spec.build_iq().query_type, InfoQueryType::Set);
    }

    #[test]
    fn test_get_group_invite_link_parse_response() {
        let jid: Jid = "120363000000000001@g.us".parse().unwrap();
        let spec = GetGroupInviteLinkIq::new(&jid, false);

        let response = NodeBuilder::new("response")
            .children([NodeBuilder::new("invite")
                .attr("code", "AbCdEfGhIjKl")
                .build()])
            .build();

        let result = spec.parse_response(&response).unwrap();
        assert_eq!(result, "https://chat.whatsapp.com/AbCdEfGhIjKl");
    }

    #[test]
    fn test_participant_change_response_parse() {
        let node = NodeBuilder::new("participant")
            .attr("jid", "1234567890@s.whatsapp.net")
            .attr("type", "200")
            .build();

        let result = ParticipantChangeResponse::try_from_node(&node).unwrap();
        assert_eq!(result.jid.user, "1234567890");
        assert_eq!(result.status, Some("200".to_string()));
    }

    // --- Tests for new IQ types ---

    #[test]
    fn test_set_group_photo_iq() {
        let jid: Jid = "120363000000000001@g.us".parse().unwrap();
        let photo = vec![0xFF, 0xD8, 0xFF, 0xE0]; // JPEG magic bytes
        let spec = SetGroupPhotoIq::new(&jid, photo.clone());
        let iq = spec.build_iq();

        assert_eq!(iq.namespace, PROFILE_PICTURE_NAMESPACE);
        assert_eq!(iq.query_type, InfoQueryType::Set);
        assert_eq!(iq.to.server, "s.whatsapp.net");
        assert_eq!(iq.target.as_ref().unwrap(), &jid);

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            let pic_node = &nodes[0];
            assert_eq!(pic_node.tag, "picture");
            assert_eq!(pic_node.attrs().optional_string("type"), Some("image"));
            // Content should be the photo bytes
            match &pic_node.content {
                Some(NodeContent::Bytes(b)) => assert_eq!(b, &photo),
                _ => panic!("expected bytes content"),
            }
        } else {
            panic!("expected nodes content");
        }
    }

    #[test]
    fn test_set_group_photo_parse_response() {
        let jid: Jid = "120363000000000001@g.us".parse().unwrap();
        let spec = SetGroupPhotoIq::new(&jid, vec![0xFF]);
        let response = NodeBuilder::new("response")
            .children([NodeBuilder::new("picture").attr("id", "12345678").build()])
            .build();
        let result = spec.parse_response(&response).unwrap();
        assert_eq!(result, "12345678");
    }

    #[test]
    fn test_delete_group_photo_iq() {
        let jid: Jid = "120363000000000001@g.us".parse().unwrap();
        let spec = DeleteGroupPhotoIq::new(&jid);
        let iq = spec.build_iq();

        assert_eq!(iq.namespace, PROFILE_PICTURE_NAMESPACE);
        assert_eq!(iq.query_type, InfoQueryType::Set);
        assert_eq!(iq.target.as_ref().unwrap(), &jid);

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            let pic_node = &nodes[0];
            assert_eq!(pic_node.tag, "picture");
            assert_eq!(pic_node.attrs().optional_string("type"), Some("image"));
            // No content = delete
            assert!(pic_node.content.is_none());
        } else {
            panic!("expected nodes content");
        }
    }

    #[test]
    fn test_join_group_with_link_iq() {
        let spec = JoinGroupWithLinkIq::new("AbCdEfGhIjKl");
        let iq = spec.build_iq();

        assert_eq!(iq.namespace, GROUP_IQ_NAMESPACE);
        assert_eq!(iq.query_type, InfoQueryType::Set);
        assert_eq!(iq.to.server, GROUP_SERVER);

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            let invite_node = &nodes[0];
            assert_eq!(invite_node.tag, "invite");
            assert_eq!(
                invite_node.attrs().optional_string("code"),
                Some("AbCdEfGhIjKl")
            );
        } else {
            panic!("expected nodes content");
        }
    }

    #[test]
    fn test_join_group_with_link_strips_prefix() {
        let spec = JoinGroupWithLinkIq::new("https://chat.whatsapp.com/AbCdEfGhIjKl");
        assert_eq!(spec.invite_code, "AbCdEfGhIjKl");
    }

    #[test]
    fn test_join_group_with_link_parse_joined() {
        let spec = JoinGroupWithLinkIq::new("AbCdEfGhIjKl");
        let response = NodeBuilder::new("response")
            .children([NodeBuilder::new("group")
                .attr("jid", "120363000000000001@g.us")
                .build()])
            .build();
        match spec.parse_response(&response).unwrap() {
            JoinGroupWithLinkResponse::Joined(jid) => {
                assert_eq!(jid.to_string(), "120363000000000001@g.us");
            }
            _ => panic!("expected Joined variant"),
        }
    }

    #[test]
    fn test_join_group_with_link_parse_pending_approval() {
        let spec = JoinGroupWithLinkIq::new("AbCdEfGhIjKl");
        let response = NodeBuilder::new("response")
            .children([NodeBuilder::new("membership_approval_request")
                .attr("jid", "120363000000000001@g.us")
                .build()])
            .build();
        match spec.parse_response(&response).unwrap() {
            JoinGroupWithLinkResponse::PendingApproval(jid) => {
                assert_eq!(jid.to_string(), "120363000000000001@g.us");
            }
            _ => panic!("expected PendingApproval variant"),
        }
    }

    #[test]
    fn test_get_group_info_from_link_iq() {
        let spec = GetGroupInfoFromLinkIq::new("https://chat.whatsapp.com/XyZ123");
        let iq = spec.build_iq();

        assert_eq!(iq.namespace, GROUP_IQ_NAMESPACE);
        assert_eq!(iq.query_type, InfoQueryType::Get);
        assert_eq!(iq.to.server, GROUP_SERVER);

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            let invite_node = &nodes[0];
            assert_eq!(invite_node.tag, "invite");
            assert_eq!(invite_node.attrs().optional_string("code"), Some("XyZ123"));
        } else {
            panic!("expected nodes content");
        }
    }

    #[test]
    fn test_join_group_with_invite_iq() {
        let group_jid: Jid = "120363000000000001@g.us".parse().unwrap();
        let inviter: Jid = "1234567890@s.whatsapp.net".parse().unwrap();
        let spec = JoinGroupWithInviteIq::new(&group_jid, &inviter, "INVITE_CODE", 1700000000);
        let iq = spec.build_iq();

        assert_eq!(iq.namespace, GROUP_IQ_NAMESPACE);
        assert_eq!(iq.query_type, InfoQueryType::Set);
        assert_eq!(iq.to, group_jid);

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            let accept_node = &nodes[0];
            assert_eq!(accept_node.tag, "accept");
            assert_eq!(
                accept_node.attrs().optional_string("code"),
                Some("INVITE_CODE")
            );
            assert_eq!(
                accept_node.attrs().optional_string("expiration"),
                Some("1700000000")
            );
            assert_eq!(
                accept_node.attrs().optional_string("admin"),
                Some("1234567890@s.whatsapp.net")
            );
        } else {
            panic!("expected nodes content");
        }
    }

    #[test]
    fn test_set_group_locked_iq() {
        let jid: Jid = "120363000000000001@g.us".parse().unwrap();

        // Test locked=true
        let spec = SetGroupLockedIq::new(&jid, true);
        let iq = spec.build_iq();
        assert_eq!(iq.namespace, GROUP_IQ_NAMESPACE);
        assert_eq!(iq.query_type, InfoQueryType::Set);
        assert_eq!(iq.to, jid);
        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            assert_eq!(nodes[0].tag, "locked");
        } else {
            panic!("expected nodes content");
        }

        // Test locked=false
        let spec = SetGroupLockedIq::new(&jid, false);
        let iq = spec.build_iq();
        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            assert_eq!(nodes[0].tag, "unlocked");
        } else {
            panic!("expected nodes content");
        }
    }

    #[test]
    fn test_set_group_announce_iq() {
        let jid: Jid = "120363000000000001@g.us".parse().unwrap();

        // Test announce=true
        let spec = SetGroupAnnounceIq::new(&jid, true);
        let iq = spec.build_iq();
        assert_eq!(iq.namespace, GROUP_IQ_NAMESPACE);
        assert_eq!(iq.query_type, InfoQueryType::Set);
        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            assert_eq!(nodes[0].tag, "announcement");
        } else {
            panic!("expected nodes content");
        }

        // Test announce=false
        let spec = SetGroupAnnounceIq::new(&jid, false);
        let iq = spec.build_iq();
        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            assert_eq!(nodes[0].tag, "not_announcement");
        } else {
            panic!("expected nodes content");
        }
    }

    #[test]
    fn test_set_group_join_approval_iq() {
        let jid: Jid = "120363000000000001@g.us".parse().unwrap();

        // Test enabled=true
        let spec = SetGroupJoinApprovalIq::new(&jid, true);
        let iq = spec.build_iq();
        assert_eq!(iq.namespace, GROUP_IQ_NAMESPACE);
        assert_eq!(iq.query_type, InfoQueryType::Set);
        assert_eq!(iq.to, jid);

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            let approval_node = &nodes[0];
            assert_eq!(approval_node.tag, "membership_approval_mode");
            let group_join = approval_node
                .get_children_by_tag("group_join")
                .next()
                .unwrap();
            assert_eq!(group_join.attrs().optional_string("state"), Some("on"));
        } else {
            panic!("expected nodes content");
        }

        // Test enabled=false
        let spec = SetGroupJoinApprovalIq::new(&jid, false);
        let iq = spec.build_iq();
        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            let approval_node = &nodes[0];
            let group_join = approval_node
                .get_children_by_tag("group_join")
                .next()
                .unwrap();
            assert_eq!(group_join.attrs().optional_string("state"), Some("off"));
        } else {
            panic!("expected nodes content");
        }
    }

    #[test]
    fn test_set_group_member_add_mode_iq() {
        let jid: Jid = "120363000000000001@g.us".parse().unwrap();

        let spec = SetGroupMemberAddModeIq::new(&jid, MemberAddMode::AdminAdd);
        let iq = spec.build_iq();
        assert_eq!(iq.namespace, GROUP_IQ_NAMESPACE);
        assert_eq!(iq.query_type, InfoQueryType::Set);

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            let mode_node = &nodes[0];
            assert_eq!(mode_node.tag, "member_add_mode");
            match &mode_node.content {
                Some(NodeContent::String(s)) => assert_eq!(s, "admin_add"),
                _ => panic!("expected string content"),
            }
        } else {
            panic!("expected nodes content");
        }

        // Test all_member_add
        let spec = SetGroupMemberAddModeIq::new(&jid, MemberAddMode::AllMemberAdd);
        let iq = spec.build_iq();
        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            match &nodes[0].content {
                Some(NodeContent::String(s)) => assert_eq!(s, "all_member_add"),
                _ => panic!("expected string content"),
            }
        } else {
            panic!("expected nodes content");
        }
    }

    #[test]
    fn test_link_group_iq() {
        let parent: Jid = "120363000000000001@g.us".parse().unwrap();
        let child: Jid = "120363000000000002@g.us".parse().unwrap();

        let spec = LinkGroupIq::new(&parent, &child);
        let iq = spec.build_iq();

        assert_eq!(iq.namespace, GROUP_IQ_NAMESPACE);
        assert_eq!(iq.query_type, InfoQueryType::Set);
        assert_eq!(iq.to, parent);

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            let links_node = &nodes[0];
            assert_eq!(links_node.tag, "links");

            let link_node = links_node.get_children_by_tag("link").next().unwrap();
            assert_eq!(
                link_node.attrs().optional_string("link_type"),
                Some("sub_group")
            );

            let group_node = link_node.get_children_by_tag("group").next().unwrap();
            assert_eq!(
                group_node.attrs().optional_string("jid"),
                Some("120363000000000002@g.us")
            );
        } else {
            panic!("expected nodes content");
        }
    }

    #[test]
    fn test_unlink_group_iq() {
        let parent: Jid = "120363000000000001@g.us".parse().unwrap();
        let child: Jid = "120363000000000002@g.us".parse().unwrap();

        let spec = UnlinkGroupIq::new(&parent, &child);
        let iq = spec.build_iq();

        assert_eq!(iq.namespace, GROUP_IQ_NAMESPACE);
        assert_eq!(iq.query_type, InfoQueryType::Set);
        assert_eq!(iq.to, parent);

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            let unlink_node = &nodes[0];
            assert_eq!(unlink_node.tag, "unlink");
            assert_eq!(
                unlink_node.attrs().optional_string("unlink_type"),
                Some("sub_group")
            );

            let group_node = unlink_node.get_children_by_tag("group").next().unwrap();
            assert_eq!(
                group_node.attrs().optional_string("jid"),
                Some("120363000000000002@g.us")
            );
        } else {
            panic!("expected nodes content");
        }
    }

    #[test]
    fn test_get_sub_groups_iq() {
        let community: Jid = "120363000000000001@g.us".parse().unwrap();
        let spec = GetSubGroupsIq::new(&community);
        let iq = spec.build_iq();

        assert_eq!(iq.namespace, GROUP_IQ_NAMESPACE);
        assert_eq!(iq.query_type, InfoQueryType::Get);
        assert_eq!(iq.to, community);

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            assert_eq!(nodes[0].tag, "sub_groups");
        } else {
            panic!("expected nodes content");
        }
    }

    #[test]
    fn test_get_sub_groups_parse_response() {
        let community: Jid = "120363000000000001@g.us".parse().unwrap();
        let spec = GetSubGroupsIq::new(&community);

        let response = NodeBuilder::new("response")
            .children([NodeBuilder::new("sub_groups")
                .children([
                    NodeBuilder::new("group")
                        .attr("jid", "120363000000000002@g.us")
                        .attr("subject", "Sub Group 1")
                        .build(),
                    NodeBuilder::new("group")
                        .attr("jid", "120363000000000003@g.us")
                        .attr("subject", "Sub Group 2")
                        .children([NodeBuilder::new("default_sub_group").build()])
                        .build(),
                ])
                .build()])
            .build();

        let result = spec.parse_response(&response).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].jid.to_string(), "120363000000000002@g.us");
        assert_eq!(result[0].subject.as_deref(), Some("Sub Group 1"));
        assert!(!result[0].is_default_sub_group);
        assert_eq!(result[1].jid.to_string(), "120363000000000003@g.us");
        assert!(result[1].is_default_sub_group);
    }

    #[test]
    fn test_get_linked_groups_participants_iq() {
        let community: Jid = "120363000000000001@g.us".parse().unwrap();
        let spec = GetLinkedGroupsParticipantsIq::new(&community);
        let iq = spec.build_iq();

        assert_eq!(iq.namespace, GROUP_IQ_NAMESPACE);
        assert_eq!(iq.query_type, InfoQueryType::Get);
        assert_eq!(iq.to, community);

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            assert_eq!(nodes[0].tag, "linked_groups_participants");
        } else {
            panic!("expected nodes content");
        }
    }

    #[test]
    fn test_get_linked_groups_participants_parse_response() {
        let community: Jid = "120363000000000001@g.us".parse().unwrap();
        let spec = GetLinkedGroupsParticipantsIq::new(&community);

        let response = NodeBuilder::new("response")
            .children([NodeBuilder::new("linked_groups_participants")
                .children([
                    NodeBuilder::new("participant")
                        .attr("jid", "1234567890@s.whatsapp.net")
                        .build(),
                    NodeBuilder::new("participant")
                        .attr("jid", "9876543210@s.whatsapp.net")
                        .build(),
                ])
                .build()])
            .build();

        let result = spec.parse_response(&response).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].to_string(), "1234567890@s.whatsapp.net");
        assert_eq!(result[1].to_string(), "9876543210@s.whatsapp.net");
    }

    #[test]
    fn test_get_group_request_participants_iq() {
        let jid: Jid = "120363000000000001@g.us".parse().unwrap();
        let spec = GetGroupRequestParticipantsIq::new(&jid);
        let iq = spec.build_iq();

        assert_eq!(iq.namespace, GROUP_IQ_NAMESPACE);
        assert_eq!(iq.query_type, InfoQueryType::Get);
        assert_eq!(iq.to, jid);

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            assert_eq!(nodes[0].tag, "membership_approval_requests");
        } else {
            panic!("expected nodes content");
        }
    }

    #[test]
    fn test_get_group_request_participants_parse_response() {
        let jid: Jid = "120363000000000001@g.us".parse().unwrap();
        let spec = GetGroupRequestParticipantsIq::new(&jid);

        let response = NodeBuilder::new("response")
            .children([NodeBuilder::new("membership_approval_requests")
                .children([
                    NodeBuilder::new("membership_approval_request")
                        .attr("jid", "1234567890@s.whatsapp.net")
                        .attr("request_time", "1700000000")
                        .build(),
                    NodeBuilder::new("membership_approval_request")
                        .attr("jid", "9876543210@s.whatsapp.net")
                        .build(),
                ])
                .build()])
            .build();

        let result = spec.parse_response(&response).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].jid.to_string(), "1234567890@s.whatsapp.net");
        assert_eq!(result[0].request_time.as_deref(), Some("1700000000"));
        assert_eq!(result[1].jid.to_string(), "9876543210@s.whatsapp.net");
        assert!(result[1].request_time.is_none());
    }

    #[test]
    fn test_update_group_request_participants_approve_iq() {
        let jid: Jid = "120363000000000001@g.us".parse().unwrap();
        let p1: Jid = "1234567890@s.whatsapp.net".parse().unwrap();
        let p2: Jid = "9876543210@s.whatsapp.net".parse().unwrap();

        let spec = UpdateGroupRequestParticipantsIq::new(
            &jid,
            &[p1, p2],
            ParticipantRequestAction::Approve,
        );
        let iq = spec.build_iq();

        assert_eq!(iq.namespace, GROUP_IQ_NAMESPACE);
        assert_eq!(iq.query_type, InfoQueryType::Set);
        assert_eq!(iq.to, jid);

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            let wrapper = &nodes[0];
            assert_eq!(wrapper.tag, "membership_requests_action");

            let approve_node = wrapper.get_children_by_tag("approve").next().unwrap();
            let participants: Vec<_> = approve_node.get_children_by_tag("participant").collect();
            assert_eq!(participants.len(), 2);
        } else {
            panic!("expected nodes content");
        }
    }

    #[test]
    fn test_update_group_request_participants_reject_iq() {
        let jid: Jid = "120363000000000001@g.us".parse().unwrap();
        let p1: Jid = "1234567890@s.whatsapp.net".parse().unwrap();

        let spec =
            UpdateGroupRequestParticipantsIq::new(&jid, &[p1], ParticipantRequestAction::Reject);
        let iq = spec.build_iq();

        if let Some(NodeContent::Nodes(nodes)) = &iq.content {
            let wrapper = &nodes[0];
            assert_eq!(wrapper.tag, "membership_requests_action");

            let reject_node = wrapper.get_children_by_tag("reject").next().unwrap();
            let participants: Vec<_> = reject_node.get_children_by_tag("participant").collect();
            assert_eq!(participants.len(), 1);
        } else {
            panic!("expected nodes content");
        }
    }

    #[test]
    fn test_update_group_request_participants_parse_response() {
        let jid: Jid = "120363000000000001@g.us".parse().unwrap();
        let spec = UpdateGroupRequestParticipantsIq::new(
            &jid,
            &["1234567890@s.whatsapp.net".parse().unwrap()],
            ParticipantRequestAction::Approve,
        );

        let response = NodeBuilder::new("response")
            .children([NodeBuilder::new("membership_requests_action")
                .children([NodeBuilder::new("approve")
                    .children([NodeBuilder::new("participant")
                        .attr("jid", "1234567890@s.whatsapp.net")
                        .attr("type", "200")
                        .build()])
                    .build()])
                .build()])
            .build();

        let result = spec.parse_response(&response).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].jid.to_string(), "1234567890@s.whatsapp.net");
        assert_eq!(result[0].status, Some("200".to_string()));
    }

    #[test]
    fn test_group_link_type_enum() {
        assert_eq!(GroupLinkType::Sub.as_str(), "sub_group");
        assert_eq!(GroupLinkType::Parent.as_str(), "parent_group");
        assert_eq!(GroupLinkType::Sibling.as_str(), "sibling_group");
        assert_eq!(
            GroupLinkType::try_from("sub_group").unwrap(),
            GroupLinkType::Sub
        );
    }

    #[test]
    fn test_group_unlink_reason_enum() {
        assert_eq!(GroupUnlinkReason::Default.as_str(), "unlink_group");
        assert_eq!(GroupUnlinkReason::DeleteParent.as_str(), "delete_parent");
    }

    #[test]
    fn test_participant_request_action_enum() {
        assert_eq!(ParticipantRequestAction::Approve.as_str(), "approve");
        assert_eq!(ParticipantRequestAction::Reject.as_str(), "reject");
    }

    #[test]
    fn test_group_link_target_roundtrip() {
        let target = GroupLinkTarget {
            jid: "120363000000000001@g.us".parse().unwrap(),
            subject: Some("Test Sub".to_string()),
            subject_time: Some("1700000000".to_string()),
            is_default_sub_group: true,
        };

        let node = target.clone().into_node();
        assert_eq!(node.tag, "group");
        assert_eq!(
            node.attrs().optional_string("jid"),
            Some("120363000000000001@g.us")
        );
        assert_eq!(node.attrs().optional_string("subject"), Some("Test Sub"));
        assert!(node.get_optional_child("default_sub_group").is_some());

        let parsed = GroupLinkTarget::try_from_node(&node).unwrap();
        assert_eq!(parsed.jid.to_string(), "120363000000000001@g.us");
        assert_eq!(parsed.subject.as_deref(), Some("Test Sub"));
        assert!(parsed.is_default_sub_group);
    }
}
