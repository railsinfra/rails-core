-- Create beta_applications table
CREATE TABLE beta_applications (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT NOT NULL,
    company TEXT NOT NULL,
    use_case TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for lookup and reporting
CREATE INDEX idx_beta_applications_email ON beta_applications(email);
CREATE INDEX idx_beta_applications_created_at ON beta_applications(created_at);
