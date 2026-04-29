-- Coalesces existing category values to a canonical form (trimmed lowercase)
-- so "Post" and "post" don't render as separate tabs in the entries list.
UPDATE entries SET category = LOWER(TRIM(category));
