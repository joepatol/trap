FROM rust:1.83 as builder

RUN apt update && apt install -y python3-full
RUN python3 -m venv /opt/venv
ENV PATH="/opt/venv/bin:$PATH"
RUN pip install maturin

WORKDIR /usr/src/myapp
COPY . .

RUN maturin build

FROM python:3.11 as application

COPY --from=builder /usr/src/myapp/target/wheels/aras-0.2.0-cp311-cp311-manylinux_2_34_x86_64.whl /app/aras/

WORKDIR /app

COPY ./test_application/ .

RUN pip install aras/aras-0.2.0-cp311-cp311-manylinux_2_34_x86_64.whl
RUN pip install uvloop
RUN pip install -r requirements.txt

EXPOSE 8080

# TODO: cli needs click installed, doesnt come with aras
CMD ["aras", "serve", "--host", "0.0.0.0", "--port", "8080", "--log-level", "INFO", "app.main:app"]
