--
-- PostgreSQL database dump
--

-- Dumped from database version 9.5.9
-- Dumped by pg_dump version 9.5.9

SET statement_timeout = 0;
SET lock_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SET check_function_bodies = false;
SET client_min_messages = warning;
SET row_security = off;

--
-- Name: plpgsql; Type: EXTENSION; Schema: -; Owner: 
--

CREATE EXTENSION IF NOT EXISTS plpgsql WITH SCHEMA pg_catalog;


--
-- Name: EXTENSION plpgsql; Type: COMMENT; Schema: -; Owner: 
--

COMMENT ON EXTENSION plpgsql IS 'PL/pgSQL procedural language';


SET search_path = public, pg_catalog;

SET default_tablespace = '';

SET default_with_oids = false;

--
-- Name: application_scene; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE application_scene (
    appid integer,
    sceneid integer
);


ALTER TABLE application_scene OWNER TO postgres;

--
-- Name: applications; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE applications (
    appid integer NOT NULL,
    origin text,
    apikey text,
    expires bigint NOT NULL,
    login boolean NOT NULL,
    temporary boolean NOT NULL
);


ALTER TABLE applications OWNER TO postgres;

--
-- Name: applications_appid_seq; Type: SEQUENCE; Schema: public; Owner: postgres
--

CREATE SEQUENCE applications_appid_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER TABLE applications_appid_seq OWNER TO postgres;

--
-- Name: applications_appid_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: postgres
--

ALTER SEQUENCE applications_appid_seq OWNED BY applications.appid;


--
-- Name: scene_data; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE scene_data (
    recordid integer NOT NULL,
    sceneid integer,
    "timestamp" bigint,
    username text,
    color integer,
    x integer,
    y integer
);


ALTER TABLE scene_data OWNER TO postgres;

--
-- Name: scene_data_recordid_seq; Type: SEQUENCE; Schema: public; Owner: postgres
--

CREATE SEQUENCE scene_data_recordid_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER TABLE scene_data_recordid_seq OWNER TO postgres;

--
-- Name: scene_data_recordid_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: postgres
--

ALTER SEQUENCE scene_data_recordid_seq OWNED BY scene_data.recordid;


--
-- Name: scenes; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE scenes (
    sceneid integer NOT NULL,
    name text,
    width integer,
    height integer
);


ALTER TABLE scenes OWNER TO postgres;

--
-- Name: scenes_sceneid_seq; Type: SEQUENCE; Schema: public; Owner: postgres
--

CREATE SEQUENCE scenes_sceneid_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER TABLE scenes_sceneid_seq OWNER TO postgres;

--
-- Name: scenes_sceneid_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: postgres
--

ALTER SEQUENCE scenes_sceneid_seq OWNED BY scenes.sceneid;


--
-- Name: appid; Type: DEFAULT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY applications ALTER COLUMN appid SET DEFAULT nextval('applications_appid_seq'::regclass);


--
-- Name: recordid; Type: DEFAULT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY scene_data ALTER COLUMN recordid SET DEFAULT nextval('scene_data_recordid_seq'::regclass);


--
-- Name: sceneid; Type: DEFAULT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY scenes ALTER COLUMN sceneid SET DEFAULT nextval('scenes_sceneid_seq'::regclass);


--
-- Name: applications_appid_seq; Type: SEQUENCE SET; Schema: public; Owner: postgres
--

SELECT pg_catalog.setval('applications_appid_seq', 1, true);

--
-- Name: scene_data_recordid_seq; Type: SEQUENCE SET; Schema: public; Owner: postgres
--

SELECT pg_catalog.setval('scene_data_recordid_seq', 1, true);

--
-- Name: scenes_sceneid_seq; Type: SEQUENCE SET; Schema: public; Owner: postgres
--

SELECT pg_catalog.setval('scenes_sceneid_seq', 1, true);


--
-- Name: application_scene_appid_sceneid_key; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY application_scene
    ADD CONSTRAINT application_scene_appid_sceneid_key UNIQUE (appid, sceneid);


--
-- Name: applications_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY applications
    ADD CONSTRAINT applications_pkey PRIMARY KEY (appid);


--
-- Name: scene_data_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY scene_data
    ADD CONSTRAINT scene_data_pkey PRIMARY KEY (recordid);


--
-- Name: scenes_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY scenes
    ADD CONSTRAINT scenes_pkey PRIMARY KEY (sceneid);


--
-- Name: applications_origin; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX applications_origin ON applications USING btree (origin);


--
-- Name: scene_data_allfields; Type: INDEX; Schema: public; Owner: postgres
--

CREATE UNIQUE INDEX scene_data_allfields ON scene_data USING btree (sceneid, "timestamp", color, x, y, username);


--
-- Name: scene_data_sceneid; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX scene_data_sceneid ON scene_data USING btree (sceneid);

ALTER TABLE scene_data CLUSTER ON scene_data_sceneid;


--
-- Name: scene_data_sceneid_ts2; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX scene_data_sceneid_ts2 ON scene_data USING btree (sceneid, "timestamp");


--
-- Name: application_scene_appid_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY application_scene
    ADD CONSTRAINT application_scene_appid_fkey FOREIGN KEY (appid) REFERENCES applications(appid) ON DELETE CASCADE;


--
-- Name: application_scene_senecid_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY application_scene
    ADD CONSTRAINT application_scene_senecid_fkey FOREIGN KEY (sceneid) REFERENCES scenes(sceneid) ON DELETE CASCADE;


--
-- Name: scene_data_sceneid_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY scene_data
    ADD CONSTRAINT scene_data_sceneid_fkey FOREIGN KEY (sceneid) REFERENCES scenes(sceneid) ON DELETE CASCADE;


--
-- Name: public; Type: ACL; Schema: -; Owner: postgres
--

REVOKE ALL ON SCHEMA public FROM PUBLIC;
REVOKE ALL ON SCHEMA public FROM postgres;
GRANT ALL ON SCHEMA public TO postgres;
GRANT ALL ON SCHEMA public TO PUBLIC;


--
-- Name: application_scene; Type: ACL; Schema: public; Owner: postgres
--

REVOKE ALL ON TABLE application_scene FROM PUBLIC;
REVOKE ALL ON TABLE application_scene FROM postgres;
GRANT ALL ON TABLE application_scene TO postgres;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE application_scene TO pbn;


--
-- Name: applications; Type: ACL; Schema: public; Owner: postgres
--

REVOKE ALL ON TABLE applications FROM PUBLIC;
REVOKE ALL ON TABLE applications FROM postgres;
GRANT ALL ON TABLE applications TO postgres;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE applications TO pbn;


--
-- Name: scene_data; Type: ACL; Schema: public; Owner: postgres
--

REVOKE ALL ON TABLE scene_data FROM PUBLIC;
REVOKE ALL ON TABLE scene_data FROM postgres;
GRANT ALL ON TABLE scene_data TO postgres;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE scene_data TO pbn;


--
-- Name: scene_data_recordid_seq; Type: ACL; Schema: public; Owner: postgres
--

REVOKE ALL ON SEQUENCE scene_data_recordid_seq FROM PUBLIC;
REVOKE ALL ON SEQUENCE scene_data_recordid_seq FROM postgres;
GRANT ALL ON SEQUENCE scene_data_recordid_seq TO postgres;
GRANT ALL ON SEQUENCE scene_data_recordid_seq TO pbn;


--
-- Name: scenes; Type: ACL; Schema: public; Owner: postgres
--

REVOKE ALL ON TABLE scenes FROM PUBLIC;
REVOKE ALL ON TABLE scenes FROM postgres;
GRANT ALL ON TABLE scenes TO postgres;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE scenes TO pbn;


--
-- Name: scenes_sceneid_seq; Type: ACL; Schema: public; Owner: postgres
--

REVOKE ALL ON SEQUENCE scenes_sceneid_seq FROM PUBLIC;
REVOKE ALL ON SEQUENCE scenes_sceneid_seq FROM postgres;
GRANT ALL ON SEQUENCE scenes_sceneid_seq TO postgres;
GRANT ALL ON SEQUENCE scenes_sceneid_seq TO pbn;


--
-- PostgreSQL database dump complete
--

