FROM aras-0.3.0-python-3.11-rustc-1.83

COPY . .

RUN pip install -r requirements.txt

EXPOSE 8080

CMD ["aras", "serve", "--host", "0.0.0.0", "--port", "8080", "--log-level", "INFO", "app.main:app"]