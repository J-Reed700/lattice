-- The Studio gave each course's notebook the name of an icon, which the
-- notebook picker printed as text; it shows an emoji, as a new one does.
UPDATE journals SET icon = '📖'
 WHERE icon = 'book-open'
   AND id IN (SELECT journal_id FROM learning_memory WHERE journal_id IS NOT NULL);
