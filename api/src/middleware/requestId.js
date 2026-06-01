const { createNamespace } = require("cls-hooked");
const { v4: uuidv4 } = require("uuid");

const ns = createNamespace("request");

const requestIdMiddleware = (req, res, next) => {
  const correlationId = req.headers["x-correlation-id"] || uuidv4();

  res.setHeader("x-correlation-id", correlationId);

  ns.run(() => {
    ns.set("correlationId", correlationId);
    next();
  });
};

module.exports = { requestIdMiddleware, ns };
