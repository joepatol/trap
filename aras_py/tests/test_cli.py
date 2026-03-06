from aras.cli import cli
from click.testing import CliRunner


def test_cli_serve() -> None:
    result = CliRunner().invoke(cli, ["serve"])
    assert result.exit_code == 2
    assert result.output.startswith("Usage")


if __name__ == "__main__":
    test_cli_serve()
