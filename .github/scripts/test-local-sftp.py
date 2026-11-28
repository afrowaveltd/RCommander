"""Run the ignored SFTP test against a disposable, loopback-only OpenSSH server."""

import getpass
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time


def run_test(listener, sshd, env):
    command = [
        "cargo", "test", "--locked", "-p", "rc-remote", "--features", "sftp",
        "--test", "sftp_local", "--", "--ignored", "--exact",
        "sftp_against_a_real_server", "--nocapture",
    ]
    with subprocess.Popen(command, env=env) as cargo:
        try:
            listener.settimeout(1)
            deadline = time.monotonic() + 300
            while time.monotonic() < deadline and cargo.poll() is None:
                try:
                    connection, _ = listener.accept()
                except TimeoutError:
                    continue
                with connection:
                    server = subprocess.run(
                        [*sshd, "-i"], stdin=connection, stdout=connection,
                        timeout=60, check=False,
                    )
                status = cargo.wait(timeout=60)
                # sshd's inetd mode can exit 255 after the client disconnects.
                # Cargo's round-trip assertions decide whether the session worked.
                if status == 0 and server.returncode not in (0, 255):
                    raise RuntimeError(f"sshd exited with status {server.returncode}")
                return status
            if cargo.poll() not in (None, 0):
                return cargo.returncode
            raise RuntimeError("the SFTP test never connected to the loopback fixture")
        finally:
            if cargo.poll() is None:
                cargo.kill()


def main():
    with tempfile.TemporaryDirectory(prefix="rcommander-sftp-") as temporary:
        base = Path(temporary)
        host_key = base / "host"
        client_key = base / "client"
        remote = base / "remote"
        remote.mkdir()
        for key in (host_key, client_key):
            subprocess.run(
                ["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", str(key)],
                check=True,
            )
        user = getpass.getuser()
        config = base / "sshd_config"
        config.write_text(
            f'HostKey "{host_key}"\n'
            f'AuthorizedKeysFile "{client_key}.pub"\n'
            f"AllowUsers {user}\n"
            "UsePAM yes\n"
            "StrictModes no\n"
            "PermitRootLogin no\n"
            "PasswordAuthentication no\n"
            "KbdInteractiveAuthentication no\n"
            "AuthenticationMethods publickey\n"
            "DisableForwarding yes\n"
            "PermitTTY no\n"
            "Subsystem sftp internal-sftp\n"
            "ForceCommand internal-sftp\n",
            encoding="utf-8",
        )
        sshd = ["/usr/sbin/sshd", "-e", "-f", str(config)]
        subprocess.run([*sshd, "-t"], check=True)
        with socket.create_server(("127.0.0.1", 0)) as listener:
            status = run_test(listener, sshd, {
                **os.environ,
                "RCOMMANDER_CONFIG_DIR": str(base / "profile"),
                "RC_SFTP_TEST_HOST": "127.0.0.1",
                "RC_SFTP_TEST_PORT": str(listener.getsockname()[1]),
                "RC_SFTP_TEST_USER": user,
                "RC_SFTP_TEST_KEY": str(client_key),
                "RC_SFTP_TEST_DIR": str(remote),
            })
        if status == 0 and any(remote.iterdir()):
            raise RuntimeError("the SFTP test left files in the remote scratch directory")
        return status


if __name__ == "__main__":
    raise SystemExit(main())
