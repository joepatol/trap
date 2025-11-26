from click.testing import CliRunner

from aras.cli import cli


def test_cli_serve() -> None:
    result = CliRunner().invoke(cli, ["serve"])
    assert result.exit_code == 0


if __name__ == "__main__":
    test_cli_serve()
