import requests
from .conftest import AppContainerInfo

from pytests.utils.arrange import ASSETS_FOLDER


def test_healthy(asgi_application: AppContainerInfo) -> None:
    response = requests.get(f"{asgi_application.uri}/health_check")
    
    assert response.status_code == 200


def test_not_found(asgi_application: AppContainerInfo) -> None:
    response = requests.get(f"{asgi_application.uri}/does_not_exist")
    
    assert response.status_code == 404
    

def test_echo_json(asgi_application: AppContainerInfo) -> None:
    data = {"Hi": "there"}
    response = requests.post(f"{asgi_application.uri}/api/basic/echo_json", json=data)
    
    assert response.status_code == 200
    assert response.json() == data


def test_echo_text(asgi_application: AppContainerInfo) -> None:
    data = "Hello"
    response = requests.get(f"{asgi_application.uri}/api/basic/echo_text?data={data}")
    
    assert response.status_code == 200
    assert response.text == data


def test_headers_ok(asgi_application: AppContainerInfo) -> None:
    response = requests.post(f"{asgi_application.uri}/api/basic/echo_json", json={"hi": "server"})
    
    assert response.status_code == 200
    assert response.headers["content-type"] == "application/json"
    assert response.headers["Content-Length"] == "15"


def test_additional_headers_ok(asgi_application: AppContainerInfo) -> None:
    response = requests.get(f"{asgi_application.uri}/api/basic/more_headers")
    
    assert response.status_code == 200
    assert response.headers["the"] == "header"


def test_app_raises_error(asgi_application: AppContainerInfo) -> None:
    response = requests.get(f"{asgi_application.uri}/api/basic/error")
    
    assert response.status_code == 500
    assert response.text == "Internal Server Error"


def test_state_is_persisted(asgi_application: AppContainerInfo) -> None:
    data = {"key": "value"}
    response = requests.patch(f"{asgi_application.uri}/api/basic/state", json=data)
    
    assert response.status_code == 204
    
    response = requests.get(f"{asgi_application.uri}/api/basic/state")
    
    assert response.status_code == 200
    assert response.text == "{'key': 'value'}"


def test_create_note(asgi_application: AppContainerInfo) -> None:
    data = {
        "title": "Test Note", 
        "content": "This is a test note", 
        "published": False, 
        "createdAt": "2021-01-01T00:00:00Z", 
        "updatedAt": "2021-01-01T00:00:00Z",
        "category": "test",
    }
    response = requests.post(f"{asgi_application.uri}/api/notes", json=data)
    
    assert response.status_code == 201
    
    note_data = response.json()["note"]
    assert note_data["title"] == data["title"]
    assert note_data["content"] == data["content"]
    assert note_data["id"] is not None
    assert note_data["createdAt"] is not None
    assert note_data["updatedAt"] is not None
    assert note_data["category"] == data["category"]
    

def test_patch_note(asgi_application: AppContainerInfo) -> None:
    data = {
        "title": "Test Note", 
        "content": "This is a test note", 
        "published": False, 
        "createdAt": "2021-01-01T00:00:00Z", 
        "updatedAt": "2021-01-01T00:00:00Z",
        "category": "test",
    }
    response = requests.post(f"{asgi_application.uri}/api/notes", json=data)
    assert response.status_code == 201
    
    note_id = response.json()["note"]["id"]
    
    data = {"title": "Updated Title", "content": "Updated Content"}
    
    response = requests.patch(f"{asgi_application.uri}/api/notes/{note_id}", json=data)
    assert response.status_code == 200


def test_upload_file(asgi_application: AppContainerInfo) -> None:
    with open(str(ASSETS_FOLDER / "basic_file.txt"), 'rb') as f1:
        with open(str(ASSETS_FOLDER / "test_file.txt")) as f2:
            response = requests.post(
                f"{asgi_application.uri}/api/files/files/",
                files=[('files', f1), ('files', f2)],
            )
    
    assert response.status_code == 200
    assert response.json() == {"file_sizes": [26, 4]}
