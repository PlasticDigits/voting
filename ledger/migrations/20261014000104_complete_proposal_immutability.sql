-- Complete opened content/identity/freeze tuple; incident/validity are writer-mutable.
CREATE OR REPLACE FUNCTION voting.guard_proposal_mutation() RETURNS trigger
LANGUAGE plpgsql SET search_path=pg_catalog,public,voting,pg_temp AS $$
BEGIN
 IF (NEW.id,NEW.chain,NEW.proposer,NEW.created_at) IS DISTINCT FROM (OLD.id,OLD.chain,OLD.proposer,OLD.created_at)
 THEN RAISE EXCEPTION 'proposal identity is immutable' USING ERRCODE='42501'; END IF;
 IF OLD.status='open' AND (NEW.title,NEW.body_html,NEW.body_canonical,NEW.body_sections,NEW.terra_height,NEW.bsc_block,NEW.revision,NEW.opened_at,NEW.status,NEW.terra_block_hash,NEW.bsc_block_hash,NEW.terra_epoch,NEW.bsc_epoch)
 IS DISTINCT FROM (OLD.title,OLD.body_html,OLD.body_canonical,OLD.body_sections,OLD.terra_height,OLD.bsc_block,OLD.revision,OLD.opened_at,OLD.status,OLD.terra_block_hash,OLD.bsc_block_hash,OLD.terra_epoch,OLD.bsc_epoch)
 THEN RAISE EXCEPTION 'opened proposal is immutable' USING ERRCODE='42501'; END IF;
 RETURN NEW;
END $$;
