app.listen(3000);
server.listen(process.env.PORT || 8080);
new Redis("redis://localhost:6379");
mongoose.connect("mongodb://localhost:27017/mydb");
