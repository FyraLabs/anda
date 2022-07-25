import { useLogto } from "@logto/react";

const Home = () => {
  const { signIn, isAuthenticated } = useLogto();

  if (isAuthenticated) {
    return <div>Signed in</div>;
  }

  return (
    <button onClick={() => signIn("http://localhost:3000/callback")}>
      Sign In
    </button>
  );
};

export default Home;
