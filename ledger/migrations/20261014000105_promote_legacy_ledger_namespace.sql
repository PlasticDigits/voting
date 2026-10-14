-- Preserve the electorate from the historical "$user" ledger namespace.
-- Only local stored ledger history is promoted; no archive-node queries/backfill.
-- SQLx runs this forward migration atomically before workers start.
SET LOCAL TIME ZONE 'UTC';
CREATE TABLE IF NOT EXISTS public.voting_legacy_ledger_archive (
    source_relation TEXT NOT NULL,
    row_fingerprint TEXT NOT NULL,
    row_data JSONB NOT NULL,
    archived_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY(source_relation,row_fingerprint)
);
CREATE TABLE IF NOT EXISTS public.voting_legacy_storage_imports (
    source_schema TEXT PRIMARY KEY,
    canonical_was_empty BOOLEAN NOT NULL,
    registration_count BIGINT NOT NULL,
    ambiguous_cw20_count BIGINT NOT NULL,
    imported_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
DO $$
DECLARE
    spec RECORD; rel TEXT; cols TEXT; key_join TEXT; id_join TEXT;
    mismatch BOOLEAN; prior BOOLEAN; canonical_empty BOOLEAN; any_source BOOLEAN := false;
    source_rows BIGINT := 0; registrations BIGINT := 0; ambiguous BIGINT := 0;
    chain_name TEXT; cursor_key TEXT; legacy_cursor BIGINT; canonical_cursor BIGINT; chosen_cursor BIGINT;
BEGIN
    PERFORM pg_advisory_xact_lock(8136,1);
    PERFORM pg_advisory_xact_lock(8136,2);
    LOCK TABLE public.voting_registrations,public.cl8y_cw20_transfers,public.cl8y_balances,
        public.cl8y_bep20_transfers,public.cl8y_bsc_balances,public.indexer_state,public.ledger_checkpoints IN ACCESS EXCLUSIVE MODE;
    SELECT EXISTS(SELECT FROM public.voting_legacy_storage_imports WHERE source_schema='voting') INTO prior;
    SELECT NOT EXISTS(SELECT FROM public.voting_registrations)
       AND NOT EXISTS(SELECT FROM public.cl8y_cw20_transfers)
       AND NOT EXISTS(SELECT FROM public.cl8y_balances)
       AND NOT EXISTS(SELECT FROM public.cl8y_bep20_transfers)
       AND NOT EXISTS(SELECT FROM public.cl8y_bsc_balances) INTO canonical_empty;

    -- Preserve every legacy row (including ambiguous event identities and tracker metadata).
    FOREACH rel IN ARRAY ARRAY['voting_registrations','cl8y_cw20_transfers','cl8y_balances','cl8y_bep20_transfers','cl8y_bsc_balances','indexer_state','_sqlx_migrations'] LOOP
        IF to_regclass('voting.'||rel) IS NULL THEN CONTINUE; END IF;
        EXECUTE format('LOCK TABLE voting.%I IN SHARE MODE',rel);
        IF prior THEN
            EXECUTE format('SELECT count(*)<>(SELECT count(*) FROM public.voting_legacy_ledger_archive WHERE source_relation=$1) FROM voting.%I',rel) INTO mismatch USING 'voting.'||rel;
            IF mismatch THEN RAISE EXCEPTION 'legacy namespace changed after promotion' USING ERRCODE='23514'; END IF;
            EXECUTE format('SELECT EXISTS(SELECT FROM voting.%I s WHERE NOT EXISTS(SELECT FROM public.voting_legacy_ledger_archive a WHERE a.source_relation=$1 AND a.row_fingerprint=encode(sha256(convert_to(to_jsonb(s)::text,''UTF8'')),''hex'')))',rel)
                INTO mismatch USING 'voting.'||rel;
            IF mismatch THEN RAISE EXCEPTION 'legacy namespace changed after promotion' USING ERRCODE='23514'; END IF;
        ELSE
            EXECUTE format('INSERT INTO public.voting_legacy_ledger_archive(source_relation,row_fingerprint,row_data) SELECT $1,encode(sha256(convert_to(to_jsonb(s)::text,''UTF8'')),''hex''),to_jsonb(s) FROM voting.%I s ON CONFLICT DO NOTHING',rel) USING 'voting.'||rel;
        END IF;
        IF rel IN ('voting_registrations','cl8y_cw20_transfers','cl8y_balances','cl8y_bep20_transfers','cl8y_bsc_balances') THEN
            EXECUTE format('SELECT count(*) FROM voting.%I',rel) INTO source_rows;
            any_source := any_source OR source_rows>0;
        END IF;
    END LOOP;
    IF prior OR NOT any_source THEN RETURN; END IF;

    -- Pre-promotion canonical metadata is evidence, never discarded/selected by higher height alone.
    FOREACH rel IN ARRAY ARRAY['indexer_state','ledger_checkpoints'] LOOP
        EXECUTE format('INSERT INTO public.voting_legacy_ledger_archive(source_relation,row_fingerprint,row_data) SELECT $1,encode(sha256(convert_to(to_jsonb(s)::text,''UTF8'')),''hex''),to_jsonb(s) FROM public.%I s ON CONFLICT DO NOTHING',rel) USING 'public.'||rel;
    END LOOP;

    -- Compare all common original values before any canonical copy. New certification
    -- columns intentionally have no legacy proof and must not be fabricated.
    FOR spec IN SELECT * FROM (VALUES
      ('voting_registrations','chain,wallet_address,registered_at_height,registered_at_time,initial_balance,status,signature_id','p.chain=l.chain AND p.wallet_address=l.wallet_address',NULL::text),
      ('cl8y_balances','wallet_address,height,balance','p.wallet_address=l.wallet_address AND p.height=l.height',NULL::text),
      ('cl8y_bsc_balances','wallet_address,bsc_block,balance','p.wallet_address=l.wallet_address AND p.bsc_block=l.bsc_block',NULL::text),
      ('cl8y_bep20_transfers','id,bsc_block,tx_hash,log_index,from_address,to_address,amount','p.tx_hash=l.tx_hash AND p.log_index=l.log_index','p.id=l.id'),
      ('cl8y_cw20_transfers','id,height,tx_hash,msg_index,from_address,to_address,amount,action','p.tx_hash=l.tx_hash AND p.msg_index=l.msg_index','p.id=l.id')
    ) AS x(relation_name,column_names,natural_join,primary_join) LOOP
        IF to_regclass('voting.'||spec.relation_name) IS NULL THEN CONTINUE; END IF;
        SELECT string_agg('p.'||quote_ident(x),',') INTO cols FROM unnest(string_to_array(spec.column_names,',')) AS x;
        SELECT string_agg('l.'||quote_ident(x),',') INTO key_join FROM unnest(string_to_array(spec.column_names,',')) AS x;
        EXECUTE format('SELECT EXISTS(SELECT FROM voting.%I l JOIN public.%I p ON (%s) WHERE ROW(%s) IS DISTINCT FROM ROW(%s))',spec.relation_name,spec.relation_name,spec.natural_join,cols,key_join) INTO mismatch;
        IF mismatch THEN RAISE EXCEPTION 'legacy canonical row conflict' USING ERRCODE='23514'; END IF;
        IF spec.primary_join IS NOT NULL THEN
            EXECUTE format('SELECT EXISTS(SELECT FROM voting.%I l JOIN public.%I p ON (%s) WHERE ROW(%s) IS DISTINCT FROM ROW(%s))',spec.relation_name,spec.relation_name,spec.primary_join,cols,key_join) INTO mismatch;
            IF mismatch THEN RAISE EXCEPTION 'legacy canonical id conflict' USING ERRCODE='23514'; END IF;
        END IF;
    END LOOP;

    IF to_regclass('voting.voting_registrations') IS NOT NULL THEN
        EXECUTE 'SELECT count(*) FROM voting.voting_registrations' INTO registrations;
        EXECUTE 'INSERT INTO public.voting_registrations(chain,wallet_address,registered_at_height,registered_at_time,initial_balance,status,signature_id,baseline_height,baseline_verified) SELECT chain,wallet_address,registered_at_height,registered_at_time,initial_balance,status,signature_id,registered_at_height,false FROM voting.voting_registrations ON CONFLICT(chain,wallet_address) DO NOTHING';
        EXECUTE 'UPDATE public.voting_registrations p SET baseline_verified=false,baseline_block_hash=NULL,baseline_height=COALESCE(p.baseline_height,p.registered_at_height) FROM voting.voting_registrations l WHERE p.chain=l.chain AND p.wallet_address=l.wallet_address';
    END IF;
    IF to_regclass('voting.cl8y_balances') IS NOT NULL THEN
        EXECUTE 'INSERT INTO public.cl8y_balances SELECT * FROM voting.cl8y_balances ON CONFLICT(wallet_address,height) DO NOTHING';
    END IF;
    IF to_regclass('voting.cl8y_bsc_balances') IS NOT NULL THEN
        EXECUTE 'INSERT INTO public.cl8y_bsc_balances SELECT * FROM voting.cl8y_bsc_balances ON CONFLICT(wallet_address,bsc_block) DO NOTHING';
    END IF;
    IF to_regclass('voting.cl8y_bep20_transfers') IS NOT NULL THEN
        EXECUTE 'INSERT INTO public.cl8y_bep20_transfers(id,bsc_block,tx_hash,log_index,from_address,to_address,amount) SELECT id,bsc_block,tx_hash,log_index,from_address,to_address,amount FROM voting.cl8y_bep20_transfers ON CONFLICT DO NOTHING';
    END IF;
    IF to_regclass('voting.cl8y_cw20_transfers') IS NOT NULL THEN
        EXECUTE 'SELECT count(*) FROM voting.cl8y_cw20_transfers l WHERE (l.tx_hash,l.msg_index) IN (SELECT tx_hash,msg_index FROM voting.cl8y_cw20_transfers GROUP BY tx_hash,msg_index HAVING count(*)>1) OR EXISTS(SELECT FROM public.cl8y_cw20_legacy_identity_conflicts c WHERE c.tx_hash=l.tx_hash AND c.msg_index=l.msg_index)' INTO ambiguous;
        -- Every excluded row remains untouched in voting and fully archived above.
        EXECUTE 'INSERT INTO public.cl8y_cw20_transfers(id,height,tx_hash,msg_index,from_address,to_address,amount,action) SELECT id,height,tx_hash,msg_index,from_address,to_address,amount,action FROM voting.cl8y_cw20_transfers l WHERE (l.tx_hash,l.msg_index) NOT IN (SELECT tx_hash,msg_index FROM voting.cl8y_cw20_transfers GROUP BY tx_hash,msg_index HAVING count(*)>1) AND NOT EXISTS(SELECT FROM public.cl8y_cw20_legacy_identity_conflicts c WHERE c.tx_hash=l.tx_hash AND c.msg_index=l.msg_index) ON CONFLICT DO NOTHING';
    END IF;
    PERFORM setval(pg_get_serial_sequence('public.cl8y_cw20_transfers','id'),GREATEST(1,COALESCE((SELECT max(id) FROM public.cl8y_cw20_transfers),0)+1),false);
    PERFORM setval(pg_get_serial_sequence('public.cl8y_bep20_transfers','id'),GREATEST(1,COALESCE((SELECT max(id) FROM public.cl8y_bep20_transfers),0)+1),false);

    FOREACH chain_name IN ARRAY ARRAY['terra','bsc'] LOOP
        cursor_key := CASE chain_name WHEN 'terra' THEN 'last_indexed_height' ELSE 'last_indexed_bsc_block' END;
        legacy_cursor := NULL;
        IF to_regclass('voting.indexer_state') IS NOT NULL THEN
            EXECUTE 'SELECT CASE WHEN value ~ ''^[0-9]+$'' THEN value::bigint ELSE NULL END FROM voting.indexer_state WHERE key=$1' INTO legacy_cursor USING cursor_key;
        END IF;
        SELECT CASE WHEN value ~ '^[0-9]+$' THEN value::bigint ELSE NULL END INTO canonical_cursor FROM public.indexer_state WHERE key=cursor_key;
        chosen_cursor := CASE WHEN canonical_empty THEN COALESCE(legacy_cursor,0) ELSE LEAST(COALESCE(canonical_cursor,0),COALESCE(legacy_cursor,0)) END;
        INSERT INTO public.indexer_state(key,value,updated_at) VALUES(cursor_key,chosen_cursor::text,clock_timestamp()) ON CONFLICT(key) DO UPDATE SET value=EXCLUDED.value,updated_at=EXCLUDED.updated_at;
        INSERT INTO public.ledger_checkpoints(chain,height,complete,canonical,degraded_reason)
            VALUES(chain_name,chosen_cursor,false,false,'legacy_electorate_requires_reconciliation')
            ON CONFLICT(chain) DO UPDATE SET height=EXCLUDED.height,block_hash='',observed_at=NULL,complete=false,canonical=false,degraded_reason=EXCLUDED.degraded_reason;
    END LOOP;
    INSERT INTO public.indexer_state(key,value,updated_at) VALUES('intents_ok','false',clock_timestamp()) ON CONFLICT(key) DO UPDATE SET value='false',updated_at=EXCLUDED.updated_at;
    INSERT INTO public.voting_legacy_storage_imports(source_schema,canonical_was_empty,registration_count,ambiguous_cw20_count) VALUES('voting',canonical_empty,registrations,ambiguous);
END $$;
-- The legacy tables and voting._sqlx_migrations are retained untouched. No poll,
-- snapshot, vote, signature, or canonical SQLx tracker row is rewritten here.
