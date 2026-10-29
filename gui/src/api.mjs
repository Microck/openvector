/** Shared same-origin transport for both workspace modes. */
export async function request(action,input,options = {}) {
  const response = await fetch(`/api/${action}`,input === undefined ? options : {
    method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(input),...options,
  });
  if (!response.ok) {
    const payload = await response.json(),error = new Error(payload.error);
    if (Object.hasOwn(payload,'code')) error.code = payload.code;
    if (Object.hasOwn(payload,'state')) error.state = payload.state;
    if (Object.hasOwn(payload,'sourceResult')) error.sourceResult = payload.sourceResult;
    if (Object.hasOwn(payload,'applicationResult')) error.applicationResult = payload.applicationResult;
    throw error;
  }
  return response.json();
}
