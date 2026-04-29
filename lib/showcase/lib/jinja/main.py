import re
from datetime import datetime
from fastapi import FastAPI, Request
from fastapi.responses import HTMLResponse
from fastapi.staticfiles import StaticFiles
from fastapi.templating import Jinja2Templates
from markupsafe import Markup

app = FastAPI()
app.mount("/static", StaticFiles(directory="static"), name="static")

templates = Jinja2Templates(directory="templates")
env = templates.env


def money(value: float, currency: str = "USD") -> str:
    symbols = {"USD": "$", "EUR": "€", "GBP": "£"}
    return f"{symbols.get(currency, '')}{value:,.2f}"


def emojify(text: str) -> str:
    mapping = {":fire:": "🔥", ":rocket:": "🚀", ":star:": "⭐"}
    for k, v in mapping.items():
        text = text.replace(k, v)
    return text


def highlight(text: str, term: str) -> Markup:
    if not term:
        return Markup.escape(text)
    pattern = re.compile(re.escape(term), re.IGNORECASE)
    safe = str(Markup.escape(text))
    return Markup(pattern.sub(lambda m: f"<mark>{m.group(0)}</mark>", safe))


env.filters["money"] = money
env.filters["emojify"] = emojify
env.filters["highlight"] = highlight
env.tests["expensive"] = lambda p: p.get("price", 0) > 100
env.globals["site_name"] = "Jinja Showcase"
env.globals["now"] = datetime.utcnow

env.trim_blocks = True
env.lstrip_blocks = True


PRODUCTS = [
    {
        "id": 1,
        "name": "Mechanical Keyboard",
        "price": 149.0,
        "tags": ["gear", "input"],
        "stock": 12,
    },
    {"id": 2, "name": "USB-C Cable", "price": 9.5, "tags": ["cable"], "stock": 0},
    {
        "id": 3,
        "name": "4K Monitor",
        "price": 489.0,
        "tags": ["display", "gear"],
        "stock": 3,
    },
    {"id": 4, "name": "Notebook", "price": 4.25, "tags": ["paper"], "stock": 87},
    {
        "id": 5,
        "name": "Ergonomic Chair",
        "price": 329.0,
        "tags": ["furniture"],
        "stock": 2,
    },
]


@app.get("/", response_class=HTMLResponse)
async def index(request: Request):
    return templates.TemplateResponse(request, "index.html")


@app.get("/products", response_class=HTMLResponse)
async def products(request: Request, q: str = "", currency: str = "USD"):
    items = [p for p in PRODUCTS if q.lower() in p["name"].lower()] if q else PRODUCTS
    return templates.TemplateResponse(
        request,
        "products.html",
        {"products": items, "query": q, "currency": currency},
    )


@app.get("/dashboard", response_class=HTMLResponse)
async def dashboard(request: Request):
    user = {"name": "Ada", "role": "admin", "email": "ada@example.com"}
    notice = "<script>alert('xss')</script> welcome back :rocket:"
    return templates.TemplateResponse(
        request,
        "dashboard.html",
        {"user": user, "notice": notice, "products": PRODUCTS},
    )
