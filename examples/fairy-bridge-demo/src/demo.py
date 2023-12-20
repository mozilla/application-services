import aiohttp
import argparse
import asyncio
import fairy_bridge
import fairy_bridge_demo

parser = argparse.ArgumentParser()
backend = parser.add_mutually_exclusive_group()
backend.add_argument("--python", action="store_true")
backend.add_argument("-c", action="store_true")
parser.add_argument("--sync", action="store_true")
parser.add_argument("--post", action="store_true")
parser.add_argument("--conn-timeout", type=int)
parser.add_argument("--timeout", type=int)
parser.add_argument("--redirect-limit", type=int)
args = parser.parse_args()

class PyBackend:
    METHOD_MAP = {
            fairy_bridge.Method.GET: "GET",
            fairy_bridge.Method.HEAD: "HEAD",
            fairy_bridge.Method.POST: "POST",
            fairy_bridge.Method.PUT: "PUT",
            fairy_bridge.Method.DELETE: "DELETE",
            fairy_bridge.Method.CONNECT: "CONNECT",
            fairy_bridge.Method.OPTIONS: "OPTIONS",
            fairy_bridge.Method.TRACE: "TRACE",
            fairy_bridge.Method.PATCH: "PATCH",
    }
    def __init__(self, settings: fairy_bridge.BackendSettings):
        self.session_kwargs = dict(
            timeout = aiohttp.ClientTimeout(
                connect = PyBackend.convert_timeout(settings.connect_timeout),
                total = PyBackend.convert_timeout(settings.timeout),
            )
        )
        self.request_kwargs = dict(
            allow_redirects = True if settings.redirect_limit > 0 else False,
            max_redirects = settings.redirect_limit,
        )

    @staticmethod
    def convert_timeout(settings_timeout):
        return None if settings_timeout is None else settings_timeout / 1000.0

    async def send_request(self, request: fairy_bridge.Request) -> fairy_bridge.Response:
        async with aiohttp.ClientSession(**self.session_kwargs) as session:
            method = self.METHOD_MAP[request.method]
            url = request.url
            kwargs = {
                "headers": request.headers,
                **self.request_kwargs
            }
            if request.body is not None:
                kwargs["data"] = request.body
            async with session.request(method, url, **kwargs) as response:
                return fairy_bridge.Response(
                    url = str(response.url),
                    status = response.status,
                    headers = response.headers,
                    body = await response.read())

settings = fairy_bridge.BackendSettings()
if args.conn_timeout is not None:
    settings.connect_timeout = args.conn_timeout
if args.timeout is not None:
    settings.timeout = args.timeout
if args.redirect_limit is not None:
    settings.redirect_limit = args.redirect_limit

if args.python:
    fairy_bridge.init_backend(PyBackend(settings))
elif args.c:
    fairy_bridge.init_backend_c(settings)
else:
    fairy_bridge.init_backend_reqwest(settings)
# Always startup an event loop.  Even if we're running in sync mode, `PyBackend` still needs it
# running.  This mimics a typical app-services setup.  Our component is sync, but the app that
# consumes it is running an async runtime.
async def run_demo():
    if args.sync:
        loop = asyncio.get_running_loop()
        # Call `uniffi_set_event_loop` so that it can run async code from the spawned thread.
        # Note: this is only needed for Python.  Both Swift and Kotlin have the concept of a global
        # runtime.
        fairy_bridge.uniffi_set_event_loop(loop)
        # Run the sync code in an executor to avoid blocking the eventloop thread
        if args.post:
            await loop.run_in_executor(None, fairy_bridge_demo.run_demo_sync_post)
        else:
            await loop.run_in_executor(None, fairy_bridge_demo.run_demo_sync)
    else:
        if args.post:
            await fairy_bridge_demo.run_demo_async_post()
        else:
            await fairy_bridge_demo.run_demo_async()
asyncio.run(run_demo())
