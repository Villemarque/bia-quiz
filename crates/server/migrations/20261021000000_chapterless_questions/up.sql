-- Chapterless questions are represented by the absence of a
-- question_chapters row. Remove the old synthetic buckets.
DELETE FROM session_chapters
WHERE chapter_id IN (SELECT id FROM chapters WHERE number BETWEEN 15 AND 17);

DELETE FROM question_chapters
WHERE chapter_id IN (SELECT id FROM chapters WHERE number BETWEEN 15 AND 17);

DELETE FROM chapters WHERE number BETWEEN 15 AND 17;
