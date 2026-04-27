-- Collaboration foundation: space members and role-based ownership.

CREATE TABLE IF NOT EXISTS collaborator_profiles (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    email TEXT,
    avatar_url TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS conversation_space_members (
    space_id TEXT NOT NULL,
    member_id TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('owner', 'editor', 'viewer')),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (space_id, member_id),
    FOREIGN KEY (space_id) REFERENCES conversation_spaces(id) ON DELETE CASCADE,
    FOREIGN KEY (member_id) REFERENCES collaborator_profiles(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_collaborator_profiles_display_name
    ON collaborator_profiles(display_name COLLATE NOCASE);

CREATE INDEX IF NOT EXISTS idx_conversation_space_members_space_role
    ON conversation_space_members(space_id, role);

CREATE INDEX IF NOT EXISTS idx_conversation_space_members_member_space
    ON conversation_space_members(member_id, space_id);

-- Local-first baseline actor used until full auth/identity arrives.
INSERT OR IGNORE INTO collaborator_profiles (
    id,
    display_name,
    email,
    avatar_url,
    created_at,
    updated_at
) VALUES (
    'member_local_owner',
    'Local Owner',
    NULL,
    NULL,
    CURRENT_TIMESTAMP,
    CURRENT_TIMESTAMP
);

-- Ensure each existing space has an owner.
INSERT OR IGNORE INTO conversation_space_members (space_id, member_id, role)
SELECT id, 'member_local_owner', 'owner'
FROM conversation_spaces;
