-- Project membership and built-in role assignments (ACC-01..03, D19).
CREATE TABLE aegis_project_member (
    project_id  uuid NOT NULL REFERENCES aegis_project (project_id) ON DELETE CASCADE,
    member_key  text NOT NULL CHECK (length(member_key) BETWEEN 6 AND 512),
    role        text NOT NULL CHECK (role IN (
        'admin', 'methodology_owner', 'project_owner', 'analyst', 'reviewer', 'viewer'
    )),
    PRIMARY KEY (project_id, member_key)
);

CREATE INDEX aegis_project_member_lookup
    ON aegis_project_member (member_key, project_id);
