"""Submit a fresh image and read the SVG downloaded by the prototype."""


def browser_trace(page, image):
    page.bring_to_front()
    previous = page.locator("#save").get_attribute("href")
    picker = page.locator("#file")
    picker.set_input_files([])
    picker.set_input_files(str(image))
    page.wait_for_function("""old => {
        const link = document.querySelector('#save');
        return !document.querySelector('#file').disabled &&
            (link.hidden || link.getAttribute('href') !== old);
    }""", arg=previous, timeout=600000)
    status = page.locator("#status").inner_text()
    link = page.locator("#save")
    if link.is_hidden() or link.get_attribute("href") == previous:
        raise RuntimeError(f"{image}: {status}")
    with page.expect_download() as pending:
        page.locator("#save").click()
    download = pending.value
    svg = download.path().read_bytes()
    download.delete()
    return float(status.split()[0]), svg
