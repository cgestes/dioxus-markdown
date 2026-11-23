export function icon(name) {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  const use = document.createElementNS(svg.namespaceURI, "use");
  svg.setAttribute("class", "icon");
  svg.setAttribute("aria-hidden", "true");
  use.setAttribute("href", "#i-" + name);
  svg.append(use);
  return svg;
}

export function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text != null) node.textContent = text;
  return node;
}

export function button(label, className, action) {
  const control = el("button", className, label);
  control.type = "button";
  control.addEventListener("click", action);
  return control;
}


export function containsPoint(node, event) {
  const b = node.getBoundingClientRect();
  return event.clientX >= b.left && event.clientX < b.right && event.clientY >= b.top && event.clientY < b.bottom;
}
