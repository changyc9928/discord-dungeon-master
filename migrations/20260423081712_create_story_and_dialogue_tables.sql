-- Add migration script here
CREATE TABLE story (
    summary VARCHAR NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (updated_at)
);

CREATE TABLE dialogues (
    dialogue VARCHAR NOT NULL,
    author_name VARCHAR NOT NULL,
    author_character VARCHAR NOT NULL,
    author_discord_id VARCHAR NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    PRIMARY KEY (updated_at)
);