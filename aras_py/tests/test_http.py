import pytest
from httpx import AsyncClient

from tests.utils.arrange import ASSETS_FOLDER


@pytest.mark.asyncio(loop_scope="session")
async def test_healthy(httpx_client: AsyncClient) -> None:
    response = await httpx_client.get("/health_check")
    assert response.status_code == 200


@pytest.mark.asyncio(loop_scope="session")
async def test_read_items(httpx_client: AsyncClient) -> None:
    response = await httpx_client.get("/api/basic/items")
    assert response.status_code == 200
    assert len(response.json()) == 3


@pytest.mark.asyncio(loop_scope="session")
async def test_read_items_query(httpx_client: AsyncClient) -> None:
    response = await httpx_client.get("/api/basic/items?skip=1")
    assert response.status_code == 200
    assert len(response.json()) == 2


@pytest.mark.asyncio(loop_scope="session")
async def test_not_found(httpx_client: AsyncClient) -> None:
    response = await httpx_client.get("/does_not_exist")
    assert response.status_code == 404
    

@pytest.mark.asyncio(loop_scope="session")
async def test_echo_json(httpx_client: AsyncClient) -> None:
    data = {"Hi": "there"}
    response = await httpx_client.post("/api/basic/echo_json", json=data)
    
    assert response.status_code == 200
    assert response.json() == data


@pytest.mark.asyncio(loop_scope="session")
async def test_echo_text(httpx_client: AsyncClient) -> None:
    data = "Hello"
    response = await httpx_client.get(f"/api/basic/echo_text?data={data}")
    
    assert response.status_code == 200
    assert response.text == data


@pytest.mark.asyncio(loop_scope="session")
async def test_headers_ok(httpx_client: AsyncClient) -> None:
    response = await httpx_client.post("/api/basic/echo_json", json={"hi": "server"})
    
    assert response.status_code == 200
    assert response.headers["content-type"] == "application/json"
    assert response.headers["Content-Length"] == "15"


@pytest.mark.asyncio(loop_scope="session")
async def test_additional_headers_ok(httpx_client: AsyncClient) -> None:
    response = await httpx_client.get("/api/basic/more_headers")
    
    assert response.status_code == 200
    assert response.headers["the"] == "header"


@pytest.mark.asyncio(loop_scope="session")
async def test_app_raises_error(httpx_client: AsyncClient) -> None:
    response = await httpx_client.get("/api/basic/error")
    
    assert response.status_code == 500
    assert response.text == "Internal Server Error"


@pytest.mark.asyncio(loop_scope="session")
async def test_state_is_persisted(httpx_client: AsyncClient) -> None:
    data = {"key": "value"}
    response = await httpx_client.patch("/api/basic/state", json=data)
    
    assert response.status_code == 204
    
    response = await httpx_client.get("/api/basic/state")
    
    assert response.status_code == 200
    assert response.text == "{'key': 'value'}"


@pytest.mark.asyncio(loop_scope="session")
async def test_create_note(httpx_client: AsyncClient) -> None:
    data = {
        "title": "Test Note", 
        "content": "This is a test note", 
        "published": False, 
        "createdAt": "2021-01-01T00:00:00Z", 
        "updatedAt": "2021-01-01T00:00:00Z",
        "category": "test",
    }
    response = await httpx_client.post("/api/notes/", json=data)
    
    assert response.status_code == 201
    
    note_data = response.json()["note"]
    assert note_data["title"] == data["title"]
    assert note_data["content"] == data["content"]
    assert note_data["id"] is not None
    assert note_data["createdAt"] is not None
    assert note_data["updatedAt"] is not None
    assert note_data["category"] == data["category"]
    

@pytest.mark.asyncio(loop_scope="session")
async def test_patch_note(httpx_client: AsyncClient) -> None:
    data = {
        "id": "666de0fd-ea39-4a52-baf9-f4901a894bed",
        "title": "Test Note", 
        "content": "This is a test note", 
        "published": False, 
        "createdAt": "2021-01-01T00:00:00Z", 
        "updatedAt": "2021-01-01T00:00:00Z",
        "category": "test",
    }
    response = await httpx_client.post("/api/notes/", json=data)
    assert response.status_code == 201
    
    data = {"title": "Updated Title", "content": "Updated Content"}
    
    response = await httpx_client.patch(f"/api/notes/666de0fd-ea39-4a52-baf9-f4901a894bed", json=data)
    assert response.status_code == 200


@pytest.mark.asyncio(loop_scope="session")
async def test_upload_file(httpx_client: AsyncClient) -> None:
    with open(str(ASSETS_FOLDER / "basic_file.txt"), 'rb') as f1:
        with open(str(ASSETS_FOLDER / "test_file.txt"), 'rb') as f2:
            response = await httpx_client.post(
                "/api/files/files/",
                files=[('files', f1), ('files', f2)],
            )
    
    assert response.status_code == 200
    assert response.json() == {"file_sizes": [26, 4]}


@pytest.mark.asyncio(loop_scope="session")
async def test_streaming_response(httpx_client: AsyncClient) -> None:
    response = await httpx_client.get("/api/stream/")

    expected_content = b"".join([b"some fake video bytes" for _ in range(10)])

    assert response.status_code == 200
    assert response.content == expected_content


@pytest.mark.asyncio(loop_scope="session")
async def test_get_html_page(httpx_client: AsyncClient) -> None:
    expected_response = """
    <html>
    <head>
        <title>Item Details</title>
        <link href="http://127.0.0.1:8080/static/styles.css" rel="stylesheet">
    </head>
    <body>
        <h1><a href="http://127.0.0.1:8080/site/items/1">Item ID: 1</a></h1>
        <span>Hello world</span>
    </body>
    </html>
    """
    
    response = await httpx_client.get("/site/items/1")
    
    assert response.status_code == 200
    assert (
        response.text.replace("\n", "").replace(" ", "") == 
        expected_response.replace("\n", "").replace(" ", "")
    )


@pytest.mark.asyncio(loop_scope="session")
async def test_get_static_image(httpx_client: AsyncClient) -> None:
    response = await httpx_client.get("/static/lightning.jpg")
    
    with open(ASSETS_FOLDER / "lightning.jpg", "rb") as f:
        expected_content = f.read()
    
    assert response.status_code == 200
    assert response.content == expected_content
