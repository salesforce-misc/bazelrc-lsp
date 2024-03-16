import subprocess
import shutil
import os
import base64 

bazelisk = shutil.which("bazel")
os.makedirs("proto/flag-dumps", exist_ok=True)

def dump_flags(version):
    result = subprocess.run(
        [bazelisk, "help", "flags-as-proto"],
        env={"USE_BAZEL_VERSION": version, "HOME": os.environ["HOME"]},
        capture_output=True,
        encoding="utf8",
    )
    if result.returncode != 0:
        raise Exception(result.stderr)
    bytes = base64.b64decode(result.stdout)
    with open(f"proto/flag-dumps/{version}.data", "wb") as f:
        f.write(bytes)

# dump_flags("6.0.0")
# dump_flags("6.1.0")
# dump_flags("6.1.1")
# dump_flags("6.1.2")
# dump_flags("6.2.0")
# dump_flags("6.2.1")
# dump_flags("6.3.0")
# dump_flags("6.3.1")
# dump_flags("6.3.2")
# dump_flags("6.4.0")
# dump_flags("6.5.0")
# dump_flags("7.0.0")
# dump_flags("7.0.1")
# dump_flags("7.0.2")
dump_flags("7.1.0")
