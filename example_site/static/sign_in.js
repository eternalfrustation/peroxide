  function showError(err) {
    console.log(err);
    document.getElementById('error-dialog').open = true;
    document.childNodes[0].classList.remove("modal-is-closing");
    document.childNodes[0].classList.add("modal-is-opening");
    document.childNodes[0].classList.add("modal-is-open");
    document.getElementById('error-message').innerHTML = err;
  }
  function closeError() {
    document.getElementById('error-dialog').open = false;
    document.childNodes[0].classList.add("modal-is-closing");
    setTimeout(() => {
      document.childNodes[0].classList.remove("modal-is-opening");
      document.childNodes[0].classList.remove("modal-is-open");
    }, 100);
  }
  htmx.on("htmx:responseError", (err) => {showError(err.detail.error)});
