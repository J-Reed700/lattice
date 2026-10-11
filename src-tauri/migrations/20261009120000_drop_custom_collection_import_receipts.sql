-- The renderer no longer migrates browser-stored collections, so nothing
-- reads or writes the import receipts.
DROP TABLE IF EXISTS custom_collection_import_receipts;
