from click.testing import CliRunner

from aras.cli import cli


def test_cli_serve() -> None:
    result = CliRunner().invoke(cli, ["serve", "aras_py.tests.utils.application.main:app"])
    assert result.exit_code == 0
