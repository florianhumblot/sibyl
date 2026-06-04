import os
import shutil
import subprocess
import sys
import time

def ensure_database_processes_limit():
    print("[db:seed] Checking database processes limit...")
    try:
        sql = "set heading off\nset feedback off\nselect value from v$parameter where name='processes';\nexit;\n"
        cmd = ["docker", "exec", "-i", "sibyl-oracle-db", "sqlplus", "-S", "sys/Or4cl3 as sysdba"]
        res = subprocess.run(cmd, input=sql, capture_output=True, text=True)
        if res.returncode != 0:
            print(f"[db:seed] Warning: sqlplus failed to check processes limit (exit {res.returncode}): {res.stderr}")
            return
        
        val_str = res.stdout.strip()
        if not val_str:
            print("[db:seed] Warning: could not parse processes limit.")
            return
            
        current_processes = int(val_str)
        print(f"[db:seed] Current database processes limit is {current_processes}")
        
        if current_processes < 500:
            print(f"[db:seed] Increasing processes limit to 500 (currently {current_processes})...")
            alter_sql = "alter system set processes=500 scope=spfile;\nexit;\n"
            subprocess.run(cmd, input=alter_sql, capture_output=True, text=True, check=True)
            
            print("[db:seed] Restarting database container to apply changes...")
            subprocess.run(["docker", "restart", "sibyl-oracle-db"], check=True)
            
            print("[db:seed] Waiting for database container to become healthy again...")
            while True:
                time.sleep(2)
                inspect_cmd = ["docker", "inspect", "--format", "{{.State.Health.Status}}", "sibyl-oracle-db"]
                health_res = subprocess.run(inspect_cmd, capture_output=True, text=True)
                if health_res.returncode == 0:
                    status = health_res.stdout.strip()
                    print(f"[db:seed] Container health status: {status}")
                    if status == "healthy":
                        break
                else:
                    print("[db:seed] Waiting for container to start...")
            
            print("[db:seed] Database container is healthy and processes limit is updated.")
        else:
            print("[db:seed] Database processes limit is already sufficient.")
    except Exception as e:
        print(f"[db:seed] Warning during processes limit check/update: {e}")


def ensure_timezone_file():
    tz_path = "etc/timezlrg_45.dat"
    if not os.path.exists(tz_path):
        print("[db:seed] timezlrg_45.dat not found on host. Attempting to copy from container...")
        try:
            cmd = ["docker", "exec", "sibyl-oracle-db", "find", "/opt/oracle/", "-name", "timezlrg_45.dat"]
            res = subprocess.run(cmd, capture_output=True, text=True)
            if res.returncode == 0 and res.stdout.strip():
                container_tz_path = res.stdout.strip().split('\n')[0]
                print(f"[db:seed] Found timezone file in container at {container_tz_path}")
                cmd_cp = ["docker", "cp", f"sibyl-oracle-db:{container_tz_path}", tz_path]
                subprocess.run(cmd_cp, check=True)
                print("[db:seed] Successfully copied timezlrg_45.dat to host.")
            else:
                print("[db:seed] Warning: Could not find timezlrg_45.dat in the container.")
        except Exception as e:
            print(f"[db:seed] Error copying timezone file: {e}")

def ensure_media_files():
    print("[db:seed] Checking and copying media files to sample schemas directory...")
    media_src = "etc/media"
    media_dst = "etc/db-sample-schemas-19.2/product_media"
    if os.path.exists(media_src) and os.path.exists(media_dst):
        count = 0
        for f in os.listdir(media_src):
            src_file = os.path.join(media_src, f)
            dst_file = os.path.join(media_dst, f)
            if os.path.isfile(src_file):
                if not os.path.exists(dst_file):
                    shutil.copy(src_file, media_dst)
                    count += 1
        if count > 0:
            print(f"[db:seed] Copied {count} media files to {media_dst}.")
        else:
            print("[db:seed] Media files are already up-to-date.")
    else:
        print("[db:seed] Warning: etc/media or product_media directory not found.")


def check_sentinel():
    cmd = ["docker", "exec", "sibyl-oracle-db", "test", "-f", "/opt/oracle/oradata/.seeded"]
    res = subprocess.run(cmd)
    return res.returncode == 0

def prepare_schemas():
    print("[db:seed] Preparing sample schema files (replacing __SUB__CWD__)...")
    base_dir = "etc/db-sample-schemas-19.2"
    count = 0
    for root, dirs, files in os.walk(base_dir):
        for f in files:
            if f.endswith('.sql'):
                path = os.path.join(root, f)
                try:
                    with open(path, 'r', encoding='utf-8', errors='ignore') as file:
                        content = file.read()
                    if '__SUB__CWD__' in content:
                        content = content.replace('__SUB__CWD__', '/db-sample-schemas')
                        with open(path, 'w', encoding='utf-8') as file:
                            file.write(content)
                        count += 1
                except Exception as e:
                    print(f"Error processing {path}: {e}")
                    sys.exit(1)
    print(f"[db:seed] Prepared {count} SQL schema files.")

def run_sqlplus(args):
    cmd = ["docker", "exec", "sibyl-oracle-db", "sqlplus"] + args
    res = subprocess.run(cmd)
    if res.returncode != 0:
        print(f"[db:seed] Error: sqlplus command failed with exit code {res.returncode}")
        sys.exit(res.returncode)

def main():
    # Pre-checks and self-healing: ensure processes limit, timezone, and media files exist
    ensure_database_processes_limit()
    ensure_timezone_file()
    ensure_media_files()

    if check_sentinel():
        print("[db:seed] Database is already seeded. Skipping seeding step.")
        sys.exit(0)

    print("[db:seed] Seeding database using 19.2 sample schemas...")
    prepare_schemas()

    # Step B: Run mksample.sql
    print("[db:seed] Executing mksample.sql in container...")
    run_sqlplus([
        "sys/Or4cl3@localhost:1521/FREEPDB1", "as", "sysdba",
        "@/db-sample-schemas/mksample.sql",
        "Or4cl3", "Or4cl3", "hr", "oe", "pm", "ix", "sh", "bi",
        "users", "temp", "/tmp/log/", "localhost:1521/FREEPDB1"
    ])

    # Step C: Run create_sandbox.sql
    print("[db:seed] Executing create_sandbox.sql in container...")
    run_sqlplus([
        "sys/Or4cl3@localhost:1521/FREEPDB1", "as", "sysdba",
        "@/create_sandbox.sql"
    ])

    # Step D: Touch sentinel
    print("[db:seed] Writing sentinel file...")
    cmd = ["docker", "exec", "sibyl-oracle-db", "touch", "/opt/oracle/oradata/.seeded"]
    subprocess.run(cmd, check=True)
    print("[db:seed] Seeding and setup completed successfully.")

if __name__ == "__main__":
    main()
