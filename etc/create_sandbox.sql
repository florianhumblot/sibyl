create user sibyl identified by Or4cl3;
grant connect, resource, unlimited tablespace, select_catalog_role to sibyl;
grant read, write on directory MEDIA_DIR to sibyl;

begin
    for r in (
        select owner, table_name
          from all_tables
         where owner in ('HR', 'OE', 'PM', 'IX', 'SH', 'BI')
           and nested = 'NO'
           and external = 'NO'
           and nvl(iot_type,'_') != 'IOT_OVERFLOW')
    loop
        begin
            execute immediate 'grant insert, select, update, delete on ' || r.owner || '.' || r.table_name || ' to sibyl';
        exception
            when others then
                dbms_output.put_line('ERROR: cannot grant access to table ' || r.owner || '.' || r.table_name || ' -- ' || substr(sqlerrm,1,200));
        end;
    end loop;

    for r in (
        select owner, view_name, read_only
          from all_views
         where owner in ('HR', 'OE', 'PM', 'IX', 'SH', 'BI'))
    loop
        begin
            execute immediate 'grant select on ' || r.owner || '.' || r.view_name || ' to sibyl';
            if r.read_only = 'N' then
                execute immediate 'grant insert, update, delete on ' || r.owner || '.' || r.view_name || ' to sibyl';
            end if;
        exception
            when others then
                dbms_output.put_line('ERROR: cannot grant access to view ' || r.owner || '.' || r.view_name || ' -- ' || substr(sqlerrm,1,200));
        end;
    end loop;

    for r in (
        select owner, object_name, object_type
          from all_objects
         where owner in ('HR', 'OE', 'PM', 'IX', 'SH', 'BI')
           and object_type in ('SEQUENCE', 'FUNCTION', 'PROCEDURE', 'PACKAGE')
           and object_name not like 'BIN$%')
    loop
        begin
            case r.object_type
            when 'SEQUENCE'  then execute immediate 'grant select  on ' || r.owner || '.' || r.object_name || ' to sibyl';
            when 'FUNCTION'  then execute immediate 'grant execute on ' || r.owner || '.' || r.object_name || ' to sibyl';
            when 'PROCEDURE' then execute immediate 'grant execute on ' || r.owner || '.' || r.object_name || ' to sibyl';
            when 'PACKAGE'   then execute immediate 'grant execute on ' || r.owner || '.' || r.object_name || ' to sibyl';
            end case;
        exception
            when others then
                dbms_output.put_line('ERROR: cannot grant access to ' || r.object_type || ' ' || r.owner || '.' || r.object_name || ' -- ' || substr(sqlerrm,1,200));
        end;
    end loop;

    for r in (
        select directory_name 
          from all_directories
         where directory_path like '%/demo/schema/%')
    loop
        execute immediate 'GRANT read, write ON DIRECTORY '||r.directory_name||' TO sibyl';
    end loop;
end;
/

GRANT SELECT ON V_$SESSION TO sibyl;

DECLARE
    name_already_used EXCEPTION; PRAGMA EXCEPTION_INIT(name_already_used, -955);
BEGIN
    EXECUTE IMMEDIATE 'CREATE SEQUENCE hr.departments_seq START WITH 280 INCREMENT BY 10';
EXCEPTION
    WHEN name_already_used THEN NULL;
END;
/

CONNECT sibyl/Or4cl3@localhost:1521/FREEPDB1

DECLARE
    name_already_used EXCEPTION; PRAGMA EXCEPTION_INIT(name_already_used, -955);
BEGIN
    EXECUTE IMMEDIATE '
        CREATE TABLE test_lobs (
            id       INTEGER GENERATED ALWAYS AS IDENTITY,
            text     CLOB,
            data     BLOB,
            ext_file BFILE
        )
    ';
EXCEPTION
    WHEN name_already_used THEN NULL;
END;
/

DECLARE
    name_already_used EXCEPTION; PRAGMA EXCEPTION_INIT(name_already_used, -955);
BEGIN
    EXECUTE IMMEDIATE '
        CREATE TABLE long_and_raw_test_data (
            id      INTEGER GENERATED ALWAYS AS IDENTITY,
            bin     RAW(100),
            text    LONG
        )
    ';
EXCEPTION
  WHEN name_already_used THEN NULL;
END;
/

DECLARE
    name_already_used EXCEPTION; PRAGMA EXCEPTION_INIT(name_already_used, -955);
BEGIN
    EXECUTE IMMEDIATE '
        CREATE TABLE test_character_data (
            id      NUMBER GENERATED ALWAYS AS IDENTITY,
            text    VARCHAR2(97),
            ntext   NVARCHAR2(99)
        )
    ';
EXCEPTION
    WHEN name_already_used THEN NULL;
END;
/

DECLARE
    name_already_used EXCEPTION; PRAGMA EXCEPTION_INIT(name_already_used, -955);
BEGIN
    EXECUTE IMMEDIATE '
        CREATE TABLE test_datetime_data (
            id      NUMBER GENERATED ALWAYS AS IDENTITY,
            dt      DATE,
            ts      TIMESTAMP(9),
            tsz     TIMESTAMP(9) WITH TIME ZONE,
            tsl     TIMESTAMP(9) WITH LOCAL TIME ZONE,
            iym     INTERVAL YEAR(9) TO MONTH,
            ids     INTERVAL DAY(8) TO SECOND(9)
        )
    ';
EXCEPTION
    WHEN name_already_used THEN NULL;
END;
/

DECLARE
    name_already_used EXCEPTION; PRAGMA EXCEPTION_INIT(name_already_used, -955);
BEGIN
    EXECUTE IMMEDIATE '
        CREATE TABLE test_large_object_data (
            id      NUMBER GENERATED ALWAYS AS IDENTITY,
            bin     BLOB,
            text    CLOB,
            ntxt    NCLOB,
            fbin    BFILE
        )
    ';
EXCEPTION
    WHEN name_already_used THEN NULL;
END;
/

DECLARE
    name_already_used EXCEPTION; PRAGMA EXCEPTION_INIT(name_already_used, -955);
BEGIN
    EXECUTE IMMEDIATE '
        CREATE TABLE test_long_raw_data (
            id      NUMBER GENERATED ALWAYS AS IDENTITY,
            bin     LONG RAW
        )
    ';
EXCEPTION
    WHEN name_already_used THEN NULL;
END;
/

DECLARE
    name_already_used EXCEPTION; PRAGMA EXCEPTION_INIT(name_already_used, -955);
BEGIN
    EXECUTE IMMEDIATE '
        CREATE TABLE test_numeric_data (
            id      NUMBER GENERATED ALWAYS AS IDENTITY,
            num     NUMBER,
            flt     BINARY_FLOAT,
            dbl     BINARY_DOUBLE
        )
    ';
EXCEPTION
    WHEN name_already_used THEN NULL;
END;
/

CREATE OR REPLACE PACKAGE SessionCustomizer AS
  TYPE prop_t IS TABLE OF VARCHAR2(256) INDEX BY VARCHAR2(120);
  PROCEDURE ParseTag (tag VARCHAR2, properties OUT prop_t);
  PROCEDURE FixSessionState (requested_tag VARCHAR2, actual_tag VARCHAR2);
END;
/

CREATE OR REPLACE PACKAGE BODY SessionCustomizer AS
  PROCEDURE ParseTag (tag VARCHAR2, properties OUT prop_t) IS
    semi_pos  INT := 0;
    name_pos  INT;
    equal_pos INT;
    name      VARCHAR2(120);
    value     VARCHAR2(256);
  BEGIN
    WHILE semi_pos <= Length(tag) LOOP
      name_pos := semi_pos + 1;
      semi_pos := InStr(tag, ';', semi_pos + 1);
      IF semi_pos = 0 THEN
        semi_pos := Length(tag) + 1;
      END IF;
      equal_pos := InStr(tag, '=', name_pos + 1);
      IF equal_pos != 0 AND equal_pos + 1 < semi_pos THEN
        name  := SubStr(tag, name_pos, equal_pos - name_pos);
        value := SubStr(tag, equal_pos + 1, semi_pos - equal_pos - 1);
        properties(name) := value;
      END IF;
    END LOOP;
  END;

  PROCEDURE FixSessionState (requested_tag VARCHAR2, actual_tag VARCHAR2) IS
    req_props prop_t;
    act_props prop_t;
    prop_name VARCHAR2(120);
  BEGIN
    ParseTag(requested_tag, req_props);
    ParseTag(actual_tag, act_props);

    prop_name := req_props.FIRST;
    WHILE prop_name IS NOT NULL LOOP
      IF NOT act_props.EXISTS(prop_name) OR act_props(prop_name) != req_props(prop_name) THEN
        EXECUTE IMMEDIATE 'ALTER SESSION SET ' || prop_name || '=''' || req_props(prop_name) || '''';
      END IF;
      prop_name := req_props.NEXT(prop_name);
    END LOOP;
  END;
END;
/

