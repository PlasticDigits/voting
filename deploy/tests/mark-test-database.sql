-- Runs only during initialization of the dedicated test container.
DO $$ BEGIN
 IF left(current_database(), 12) <> 'voting_test_' THEN
  RAISE EXCEPTION 'refuse marking a non-test database';
 END IF;
 EXECUTE format('COMMENT ON DATABASE %I IS %L', current_database(), 'cl8y-voting-ephemeral-test');
END $$;
